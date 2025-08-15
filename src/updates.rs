use std::error::Error;

use self_update::{self, cargo_crate_version, get_target, update::Release};

pub fn update(branch: String) -> Result<(), Box<dyn Error + Send + Sync>> {
    self_update::backends::github::Update::configure()
        .repo_owner("Open-Resin-Alliance")
        .repo_name("Odyssey")
        .bin_name("odyssey")
        .bin_path_in_archive("{{ bin }}")
        .target(get_target())
        .target_version_tag(branch.as_str())
        .show_download_progress(true)
        .no_confirm(true)
        .current_version(cargo_crate_version!())
        .build()?
        .update()?;
    Ok(())
}

pub fn get_releases() -> Result<Vec<Release>, Box<dyn Error + Send + Sync>> {
    Ok(self_update::backends::github::ReleaseList::configure()
        .repo_owner("Open-Resin-Alliance")
        .repo_name("Odyssey")
        .with_target(get_target())
        .build()?
        .fetch()?)
}
