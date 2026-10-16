//! `init` subcommand

use abscissa_core::{Command, Runnable, Shutdown, status_err};
use anyhow::{Result, bail};
use bytesize::ByteSize;
use conflate::{Merge, MergeFrom};
use dialoguer::Password;
use serde::{Deserialize, Serialize};

use crate::{
    Application, RUSTIC_APP, RusticConfig,
    repository::{OpenRepo, Repo},
};

use rustic_core::{
    ConfigOptions, CredentialOptions, Credentials, KeyOptions, repofile::ChunkerPolynomial,
};

/// `init` subcommand
#[derive(clap::Parser, Command, Debug)]
pub(crate) struct InitCmd {
    /// initialize hot repository for existing cold repository
    #[clap(long)]
    hot_only: bool,

    /// Key options
    #[clap(flatten, next_help_heading = "Key options")]
    key_opts: KeyOptions,

    /// Config options
    #[clap(flatten, next_help_heading = "Config options")]
    init_opts: InitOptions,
}

/// Options for initializing a repository
#[derive(Clone, Default, Debug, clap::Parser, Serialize, Deserialize, Merge)]
#[serde(default, rename_all = "kebab-case", deny_unknown_fields)]
pub struct InitOptions {
    /// Copy the chunker parameters from the repository of this profile to allow copying snapshots between both.
    /// Overrides the other chunker options
    #[clap(long, value_name = "PROFILE")]
    #[merge(strategy=conflate::option::overwrite_none)]
    copy_chunker_params_from: Option<String>,

    #[clap(flatten)]
    #[serde(flatten)]
    config_opts: ConfigOptions,
}

impl Runnable for InitCmd {
    fn run(&self) {
        if let Err(err) = RUSTIC_APP
            .config()
            .repository
            .run(|repo| self.inner_run(repo))
        {
            status_err!("{}", err);
            RUSTIC_APP.shutdown(Shutdown::Crash);
        };
    }
}

impl InitCmd {
    fn inner_run(&self, repo: Repo) -> Result<()> {
        let config = RUSTIC_APP.config();

        // Handle dry-run mode
        if config.global.dry_run {
            bail!(
                "cannot initialize repository {} in dry-run mode!",
                repo.name
            );
        }

        if self.hot_only {
            if config.repository.be.repo_hot.is_none() {
                bail!("please specify a hot repository");
            }
            let repo = repo
                .open_with(&config.repository.credential_opts, |repo, credentials| {
                    repo.open_only_cold(credentials)
                })?;
            repo.init_hot()?;
            repo.repair_hotcold_except_packs(config.global.dry_run)?;
            repo.repair_hotcold_packs(config.global.dry_run)?;

            return Ok(());
        }

        // Note: This is again checked in init(), however we want to inform
        // users before they are prompted to enter a password
        if repo.config_id()?.is_some() {
            bail!("Config file already exists. Aborting.");
        }

        let _ = init(
            repo,
            &config.repository.credential_opts,
            &self.key_opts,
            &self.init_opts,
        )?;
        Ok(())
    }
}

/// Initialize repository
///
/// # Arguments
///
/// * `repo` - Repository to initialize
/// * `credential_opts` - Credential options
/// * `key_opts` - Key options (only used when generating a new key)
/// * `init_opts` - Init options, merged with the `[init]` section of the profile
///
/// # Errors
///
/// * If getting the credentials from the options fails
/// * If the repository to copy the chunker parameters from cannot be opened
///
/// # Returns
///
/// Returns the initialized repository
pub(crate) fn init(
    repo: Repo,
    credential_opts: &CredentialOptions,
    key_opts: &KeyOptions,
    init_opts: &InitOptions,
) -> Result<OpenRepo> {
    let mut opts = init_opts
        .clone()
        .merge_from(RUSTIC_APP.config().init.clone());
    if let Some(profile) = &opts.copy_chunker_params_from {
        let source = RusticConfig::from_profile(profile)?
            .repository
            .run_open(|repo| Ok(repo.config().clone()))?;
        opts.config_opts = opts
            .config_opts
            .set_chunker(source.chunker())
            .set_chunker_polynomial(source.chunker_polynomial.parse::<ChunkerPolynomial>()?)
            .set_chunk_size(ByteSize(source.chunk_size() as u64))
            .set_chunk_min_size(ByteSize(source.chunk_min_size() as u64))
            .set_chunk_max_size(ByteSize(source.chunk_max_size() as u64));
    }
    let pass = init_credentials(credential_opts)?;
    Ok(repo.0.init(&pass, key_opts, &opts.config_opts)?)
}

pub(crate) fn init_credentials(credential_opts: &CredentialOptions) -> Result<Credentials> {
    let credentials = credential_opts.credentials()?.unwrap_or_else(|| {
        match Password::new()
            .with_prompt("enter password for new key")
            .allow_empty_password(true)
            .with_confirmation("confirm password", "passwords do not match")
            .interact()
        {
            Ok(pass) => Credentials::Password(pass),
            Err(err) => {
                status_err!("{}", err);
                RUSTIC_APP.shutdown(Shutdown::Crash);
            }
        }
    });

    Ok(credentials)
}
