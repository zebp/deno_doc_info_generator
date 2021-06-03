use std::{
    collections::HashMap,
    io::{self, Cursor, Read},
    ops::{Deref, DerefMut},
    path::Path,
    sync::Arc,
};

use deno_doc::{parser::DocFileLoader, DocError};
use flate2::read::GzDecoder;
use futures::future::LocalBoxFuture;
use swc_ecmascript::parser::{Syntax, TsConfig};
use tar::{Archive, Entry};
use tokio::sync::Mutex;

/// An archive containing the files of a Deno module.
pub struct DenoArchive {
    pub module_name: String,
    pub version: String,
    pub archive: Archive<Cursor<Vec<u8>>>,
}

impl DenoArchive {
    /// Creates a [DenoArchive] from a reader containing a tar.gz file.
    pub fn from_reader<R>(module_name: String, version: String, reader: R) -> io::Result<Self>
    where
        R: Read,
    {
        let mut buffer = Vec::new();
        let mut decoder = GzDecoder::new(reader);
        decoder.read_to_end(&mut buffer)?;

        Ok(Self {
            module_name,
            version,
            archive: Archive::new(Cursor::new(buffer)),
        })
    }

    pub fn entries(&mut self) -> io::Result<impl Iterator<Item = io::Result<DenoEntry<'_>>>> {
        let iterator = self
            .archive
            .entries()?
            .skip(1)
            .map(|e| e.map(|e| DenoEntry(e)));
        Ok(iterator)
    }

    /// Gets the root directory in the archive.
    pub fn root_directory(&mut self) -> io::Result<Option<String>> {
        let ret = match self.archive.entries()?.skip(1).next() {
            Some(res) => {
                let entry = res?;
                Ok(entry.path()?.to_str().map(String::from))
            }
            None => Ok(None),
        };

        replace_with::replace_with_or_abort(&mut self.archive, |archive| {
            let mut reader = archive.into_inner();

            // Rewinds the reader so we can read it again.
            reader.set_position(0);

            Archive::new(reader)
        });

        ret
    }
}

pub struct DenoArchiveLoader(Arc<Mutex<DenoArchiveInner>>);

struct DenoArchiveInner {
    // A mutex is used because the loading is a asynchronous.
    archive: DenoArchive,
    // A cache for files inside of the deno archive so they don't need to be read to again.
    cache: HashMap<String, String>,
}

impl From<DenoArchive> for DenoArchiveLoader {
    fn from(archive: DenoArchive) -> Self {
        Self(Arc::new(Mutex::new(DenoArchiveInner {
            archive,
            cache: HashMap::default(),
        })))
    }
}

impl DocFileLoader for DenoArchiveLoader {
    fn resolve(&self, specifier: &str, referrer: &str) -> Result<String, DocError> {
        if specifier.starts_with("https://") {
            return Ok(specifier.to_string());
        }

        log::debug!("Resolving {} referred to by {}", specifier, referrer);
        todo!()
    }

    fn load_source_code(
        &self,
        specifier: &str,
    ) -> LocalBoxFuture<Result<(Syntax, String), DocError>> {
        log::debug!("Loading {} from deno archive", specifier);

        let this = self.0.clone();
        let specifier = specifier.to_string();
        Box::pin(async move {
            let mut inner = this.lock().await;
            let specifier_path = Path::new(&specifier);

            let source = inner.cache.get(&specifier);
            let had_source = source.is_some();
            let source = match source {
                Some(value) => value.clone(),
                None => {
                    let mut entry: DenoEntry<'_> = inner
                        .archive
                        .entries()
                        .map_err(DocError::Io)?
                        .filter_map(Result::ok)
                        .find(|entry| {
                            entry
                                .path()
                                .map(|x| x.as_ref() == specifier_path)
                                .unwrap_or(false)
                        })
                        .ok_or(DocError::Resolve(format!("{} not in archive", &specifier)))?;

                    let mut buffer = Vec::with_capacity(entry.size() as usize);
                    entry.read_to_end(&mut buffer).unwrap();
                    String::from_utf8(buffer).unwrap()
                }
            };

            if !had_source {
                inner.cache.insert(specifier, source.clone());
            }

            Ok((Syntax::Typescript(TsConfig::default()), source))
        })
    }
}

/// A file in a [DenoArchive].
pub struct DenoEntry<'archive>(Entry<'archive, Cursor<Vec<u8>>>);

impl<'archive> Deref for DenoEntry<'archive> {
    type Target = Entry<'archive, Cursor<Vec<u8>>>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<'archive> DerefMut for DenoEntry<'archive> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
