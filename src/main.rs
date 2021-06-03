mod deno_archive;
mod fetch;

use std::{env, io::Cursor};

use deno_archive::{DenoArchive, DenoArchiveLoader};
use deno_doc::DocParser;
use reqwest::{redirect::Policy, ClientBuilder};

use crate::fetch::FetchError;

#[cfg(not(debug_assertions))]
const DEFAULT_LOG_FILTER: &'static str = "deno_doc_info_generator=info,error";
#[cfg(debug_assertions)]
const DEFAULT_LOG_FILTER: &'static str = "deno_doc_info_generator=debug";

#[tokio::main]
async fn main() {
    // Sets the default logger predicate.
    env::set_var(
        "RUST_LOG",
        env::var("RUST_LOG").unwrap_or(DEFAULT_LOG_FILTER.into()),
    );

    pretty_env_logger::init();

    let client = ClientBuilder::new()
        .redirect(Policy::default())
        .user_agent("deno-doc-info-generator")
        .build()
        .unwrap();

    // TODO: make this configurable potentially through an env var.
    let module = "channo";

    let versions = match fetch::fetch_versions_for_module(&client, module).await {
        Ok(v) => v,
        Err(FetchError::MetadataNotPresent) => return log::error!("Module not found"),
        Err(e) => return log::error!("{}", e),
    };
    let version_metadata =
        match fetch::fetch_version_metadata(&client, module, &versions.latest).await {
            Ok(v) => v,
            Err(FetchError::MetadataNotPresent) => return log::error!("Version not found"),
            Err(e) => return log::error!("{}", e),
        };

    let url = version_metadata.upload_options.tarball_url().unwrap();
    let bytes = client.get(url).send().await.unwrap().bytes().await.unwrap();
    let reader = Cursor::new(bytes.to_vec());

    let mut archive = DenoArchive::from_reader("channo".into(), "0.1.1".into(), reader)
        .expect("unable to decode archive");
    let root_directory = archive.root_directory().unwrap().unwrap();

    log::debug!("Root directory of archive is \"{}\"", &root_directory);

    let file_loader: DenoArchiveLoader = archive.into();
    let doc_parser = DocParser::new(Box::new(file_loader), false);

    let res = doc_parser
        .parse(&format!("{}/mod.ts", root_directory))
        .await
        .unwrap();
    log::debug!("Found {} doc items", res.len());
}
