// Copyright (c) 2018-present, Facebook, Inc.
// All Rights Reserved.
//
// This software may be used and distributed according to the terms of the
// GNU General Public License version 2 or any later version.

use std::cmp::min;
use std::fs;
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use clap::{App, Arg, ArgMatches};
use failure::{err_msg, Error, Result, ResultExt};
use futures::{Future, IntoFuture};
use futures_ext::FutureExt;
use panichandler::{self, Fate};
use scuba_ext::ScubaSampleBuilder;
use slog::{Drain, Logger};
use sloggers::terminal::{Destination, TerminalLoggerBuilder};
use sloggers::types::{Format, Severity, SourceLocation};
use sloggers::Build;
use sshrelay::SshEnvVars;
use tracing::TraceContext;
use upload_trace::{manifold_thrift::thrift::RequestContext, UploadTrace};
use uuid::Uuid;

use cachelib;
use slog_glog_fmt::default_drain as glog_drain;

use blobrepo::BlobRepo;
use blobrepo_factory::open_blobrepo;
use changesets::{SqlChangesets, SqlConstructors};
use context::CoreContext;
use metaconfig_parser::RepoConfigs;
use metaconfig_types::{
    ManifoldArgs, MysqlBlobstoreArgs, RemoteBlobstoreArgs, RepoConfig, RepoType,
};
use mononoke_types::RepositoryId;

const CACHE_ARGS: &[(&str, &str)] = &[
    ("blob-cache-size", "override size of the blob cache"),
    (
        "presence-cache-size",
        "override size of the blob presence cache",
    ),
    (
        "changesets-cache-size",
        "override size of the changesets cache",
    ),
    (
        "filenodes-cache-size",
        "override size of the filenodes cache",
    ),
    (
        "idmapping-cache-size",
        "override size of the bonsai/hg mapping cache",
    ),
    (
        "content-sha1-cache-size",
        "override size of the content SHA1 cache",
    ),
];

pub struct MononokeApp {
    /// Whether to redirect writes to non-production by default. Note that this isn't (yet)
    /// foolproof.
    pub safe_writes: bool,
    /// Whether to hide advanced Manifold configuration from help. Note that the arguments will
    /// still be available, just not displayed in help.
    pub hide_advanced_args: bool,
    /// Whether this tool can deal with local instances (which are very useful for testing).
    pub local_instances: bool,
    /// Whether to use glog by default.
    pub default_glog: bool,
}

impl MononokeApp {
    /// Create a new Mononoke-based CLI tool. The `safe_writes` option changes some defaults to
    /// avoid production writes. (But it isn't foolproof -- please fix any options that are
    /// missing).
    pub fn build<'a, 'b, S: Into<String>>(self, name: S) -> App<'a, 'b> {
        let default_manifold_prefix = if self.safe_writes {
            "mononoke_test"
        } else {
            ""
        };

        let name = name.into();
        let default_log = if self.default_glog { "glog" } else { "compact" };

        let mut app = App::new(name)
            .args_from_usage(
                r#"
                -d, --debug 'print debug output'
                "#,
            )
            .arg(
                Arg::with_name("repo-id")
                    .long("repo-id")
                    // This is an old form that some consumers use
                    .alias("repo_id")
                    .value_name("ID")
                    .default_value("0")
                    .help("numeric ID of repository")
            )
            .arg(
                Arg::with_name("log-style")
                    .short("l")
                    .long("log-style")
                    .value_name("STYLE")
                    .possible_values(&["compact", "glog"])
                    .default_value(default_log)
                    .help("log style to use for output")
            )
            .arg(
                Arg::with_name("panic-fate")
                    .long("panic-fate")
                    .value_name("PANIC_FATE")
                    .possible_values(&["continue", "exit", "abort"])
                    .default_value("abort")
                    .help("fate of the process when a panic happens")
            )
            .arg(
                Arg::with_name("mononoke-config-path")
                    .long("mononoke-config-path")
                    .value_name("MONONOKE_CONFIG_PATH")
                    .help("Path to the Mononoke configs")
            )

            // Manifold-specific arguments
            .arg(
                Arg::with_name("manifold-bucket")
                    .long("manifold-bucket")
                    .value_name("BUCKET")
                    .default_value("mononoke_prod")
                    .help("manifold bucket"),
            )
            .arg(
                Arg::with_name("manifold-prefix")
                    .long("manifold-prefix")
                    .value_name("PREFIX")
                    .default_value(default_manifold_prefix)
                    .help("manifold prefix"),
            )
            .arg(
                Arg::with_name("mysql-blobstore-shardmap")
                    .long("mysql-blobstore-shardmap")
                    .value_name("SHARDMAP")
                    .help("mysql blobstore shardmap, if provided mysql is used instead of \
                           manifold"),
            )
            .arg(
                Arg::with_name("mysql-blobstore-shard-num")
                    .long("mysql-blobstore-shard-num")
                    .value_name("SHARD_NUM")
                    .default_value("100")
                    .help("mysql blobstore shard num"),
            )

            .arg(
                Arg::with_name("db-address")
                    .long("db-address")
                    .value_name("ADDRESS")
                    .default_value("xdb.mononoke_production")
                    .help("database address"),
            )
            .arg(
                Arg::with_name("filenode-shards")
                .long("filenode-shards").value_name("SHARD_COUNT").help("number of shards to spread filenodes across")
            );

        app = add_myrouter_args(app);
        app = add_cachelib_args(app, self.hide_advanced_args);

        if self.local_instances {
            app = app
                .arg(
                    Arg::with_name("blobstore")
                        .long("blobstore")
                        .value_name("TYPE")
                        .possible_values(&["files", "rocksdb", "manifold"])
                        .default_value("manifold")
                        .help("blobstore type"),
                )
                .arg(
                    Arg::with_name("data-dir")
                        .long("data-dir")
                        .value_name("DIR")
                        .help("local data directory (used for local blobstores)"),
                );
        }

        app
    }
}

pub fn get_logger<'a>(matches: &ArgMatches<'a>) -> Logger {
    // Set the panic handler up here. Not really relevent to logger other than it emits output
    // when things go wrong. This writes directly to stderr as coredumper expects.
    let fate = match matches
        .value_of("panic-fate")
        .expect("no default on panic-fate")
    {
        "none" => None,
        "continue" => Some(Fate::Continue),
        "exit" => Some(Fate::Exit(101)),
        "abort" => Some(Fate::Abort),
        bad => panic!("bad panic-fate {}", bad),
    };
    if let Some(fate) = fate {
        panichandler::set_panichandler(fate);
    }

    let severity = if matches.is_present("debug") {
        Severity::Debug
    } else {
        Severity::Info
    };

    let log_style = matches
        .value_of("log-style")
        .expect("default style is always specified");
    match log_style {
        "glog" => {
            let drain = glog_drain().filter_level(severity.as_level()).fuse();
            Logger::root(drain, o![])
        }
        "compact" => {
            let mut builder = TerminalLoggerBuilder::new();
            builder.destination(Destination::Stderr);
            builder.level(severity);
            builder.format(Format::Compact);
            builder.source_location(SourceLocation::None);

            builder.build().unwrap()
        }
        _other => unreachable!("unknown log style"),
    }
}

pub fn get_core_context<'a>(matches: &ArgMatches<'a>) -> CoreContext {
    let session_uuid = Uuid::new_v4();
    let trace = TraceContext::new(session_uuid, Instant::now());
    let logger = get_logger(&matches);

    CoreContext::new(
        session_uuid,
        logger.clone(),
        ScubaSampleBuilder::with_discard(),
        None,
        trace.clone(),
        None,
        SshEnvVars::default(),
    )
}

pub fn upload_and_show_trace(ctx: CoreContext) -> impl Future<Item = (), Error = !> {
    if !ctx.trace().is_enabled() {
        debug!(ctx.logger(), "Trace is disabled");
        return Ok(()).into_future().left_future();
    }

    let rc = RequestContext {
        bucketName: "mononoke_prod".into(),
        apiKey: "".into(),
        ..Default::default()
    };

    ctx.trace()
        .upload_to_manifold(rc)
        .then(move |upload_res| {
            match upload_res {
                Err(err) => debug!(ctx.logger(), "Failed to upload trace: {:#?}", err),
                Ok(()) => debug!(
                    ctx.logger(),
                    "Trace taken: https://our.intern.facebook.com/intern/mononoke/trace/{}",
                    ctx.trace().id()
                ),
            }
            Ok(())
        })
        .right_future()
}

pub fn get_repo_id<'a>(matches: &ArgMatches<'a>) -> RepositoryId {
    let repo_id = matches
        .value_of("repo-id")
        .unwrap()
        .parse::<u32>()
        .expect("expected repository ID to be a u32");
    RepositoryId::new(repo_id as i32)
}

pub fn open_sql<T>(matches: &ArgMatches, name: &'static str) -> Result<T>
where
    T: SqlConstructors,
{
    let (_, config) = get_config(matches)?;
    match config.repotype {
        RepoType::BlobFiles(ref data_dir)
        | RepoType::BlobRocks(ref data_dir)
        | RepoType::BlobSqlite(ref data_dir) => T::with_sqlite_path(data_dir.join(name)),
        RepoType::BlobRemote { ref db_address, .. } => {
            let myrouter_port =
                parse_myrouter_port(matches).expect("myrouter port provided is not provided");
            Ok(T::with_myrouter(&db_address, myrouter_port))
        }
    }
}

pub fn open_sql_changesets(matches: &ArgMatches) -> Result<SqlChangesets> {
    open_sql::<SqlChangesets>(matches, "changesets")
}

/// Create a new `BlobRepo` -- for local instances, expect its contents to be empty.
#[inline]
pub fn create_repo<'a>(
    logger: &Logger,
    matches: &ArgMatches<'a>,
) -> impl Future<Item = BlobRepo, Error = Error> {
    open_repo_internal(logger, matches, true)
}

/// Open an existing `BlobRepo` -- for local instances, expect contents to already be there.
#[inline]
pub fn open_repo<'a>(
    logger: &Logger,
    matches: &ArgMatches<'a>,
) -> impl Future<Item = BlobRepo, Error = Error> {
    open_repo_internal(logger, matches, false)
}

pub fn setup_repo_dir<P: AsRef<Path>>(data_dir: P, create: bool) -> Result<()> {
    let data_dir = data_dir.as_ref();

    if !data_dir.is_dir() {
        bail_msg!("{:?} does not exist or is not a directory", data_dir);
    }

    for subdir in &["blobs"] {
        let subdir = data_dir.join(subdir);

        if subdir.exists() && !subdir.is_dir() {
            bail_msg!("{:?} already exists and is not a directory", subdir);
        }

        if create {
            if subdir.exists() {
                let content: Vec<_> = subdir.read_dir()?.collect();
                if !content.is_empty() {
                    bail_msg!(
                        "{:?} already exists and is not empty: {:?}",
                        subdir,
                        content
                    );
                }
            } else {
                fs::create_dir(&subdir)
                    .with_context(|_| format!("failed to create subdirectory {:?}", subdir))?;
            }
        }
    }
    Ok(())
}

pub fn add_cachelib_args<'a, 'b>(app: App<'a, 'b>, hide_advanced_args: bool) -> App<'a, 'b> {
    let cache_args: Vec<_> = CACHE_ARGS
        .iter()
        .map(|(flag, help)| {
            // XXX figure out a way to get default values in here -- note that .default_value
            // takes a &'a str, so we may need to have MononokeApp own it or similar.
            Arg::with_name(flag)
                .long(flag)
                .value_name("SIZE")
                .hidden(hide_advanced_args)
                .help(help)
        })
        .collect();

    app.arg(Arg::from_usage(
            "--cache-size-gb [SIZE] 'size of the cachelib cache, in GiB'",
    ))
    .arg(Arg::from_usage(
            "--use-tupperware-shrinker 'Use the Tupperware-aware cache shrinker to avoid OOM'"
    ))
    .arg(Arg::from_usage(
            "--max-process-size [SIZE] 'process size at which cachelib will shrink, in GiB'"
    ))
    .arg(Arg::from_usage(
            "--min-process-size [SIZE] 'process size at which cachelib will grow back to cache-size-gb, in GiB'"
    ))
    .arg(Arg::from_usage(
            "--with-content-sha1-cache  '[Mononoke API Server only] enable content SHA1 cache'"
    ))
    .args_from_usage(
        r#"
        --do-not-init-cachelib 'do not init cachelib (useful for tests)'
        "#,
    )
    .args(&cache_args)
}

// TODO: (jsgf) T32777804 make the dependency between cachelib and blobrepo more visible
pub fn init_cachelib<'a>(matches: &ArgMatches<'a>) {
    if matches.is_present("do-not-init-cachelib") {
        return;
    }
    let cache_size_gb = matches
        .value_of("cache-size-gb")
        .unwrap_or("20")
        .parse::<usize>()
        .unwrap();

    // Millions of lookups per second
    let lock_power = 10;
    // Assume 200 bytes average cache item size and compute bucketsPower
    let expected_item_size_bytes = 200;
    let cache_size_bytes = cache_size_gb * 1024 * 1024 * 1024;
    let item_count = cache_size_bytes / expected_item_size_bytes;

    // Because `bucket_count` is a power of 2, bucket_count.trailing_zeros() is log2(bucket_count)
    let bucket_count = item_count
        .checked_next_power_of_two()
        .expect("Cache has too many objects to fit a `usize`?!?");
    let buckets_power = min(bucket_count.trailing_zeros() + 1 as u32, 32);

    let mut cache_config = cachelib::LruCacheConfig::new(cache_size_bytes)
        .set_pool_rebalance(cachelib::PoolRebalanceConfig {
            interval: Duration::new(300, 0),
            strategy: cachelib::RebalanceStrategy::HitsPerSlab {
                // A small increase in hit ratio is desired
                diff_ratio: 0.05,
                min_retained_slabs: 1,
                // Objects newer than 30 seconds old might be about to become interesting
                min_tail_age: Duration::new(30, 0),
                ignore_untouched_slabs: false,
            },
        })
        .set_access_config(buckets_power, lock_power);

    if matches.is_present("use-tupperware-shrinker") {
        if matches.is_present("max-process-size") || matches.is_present("min-process-size") {
            panic!("Can't use both Tupperware shrinker and manually configured shrinker");
        }

        cache_config = cache_config.set_tupperware_shrinker();
    } else {
        let opt_max_process_size_gib = matches.value_of("max-process-size").map(str::parse::<u32>);
        let opt_min_process_size_gib = matches.value_of("min-process-size").map(str::parse::<u32>);

        match (opt_max_process_size_gib, opt_min_process_size_gib) {
            (None, None) => (),
            (Some(_), None) | (None, Some(_)) => {
                panic!("If setting process size limits, must set both max and min")
            }
            (Some(max), Some(min)) => {
                cache_config = cache_config.set_shrinker(cachelib::ShrinkMonitor {
                    shrinker_type: cachelib::ShrinkMonitorType::ResidentSize {
                        max_process_size_gib: max.unwrap(),
                        min_process_size_gib: min.unwrap(),
                    },
                    interval: Duration::new(10, 0),
                    max_resize_per_iteration_percent: 25,
                    max_removed_percent: 50,
                    strategy: cachelib::RebalanceStrategy::HitsPerSlab {
                        // A small increase in hit ratio is desired
                        diff_ratio: 0.05,
                        min_retained_slabs: 1,
                        // Objects newer than 30 seconds old might be about to become interesting
                        min_tail_age: Duration::new(30, 0),
                        ignore_untouched_slabs: false,
                    },
                });
            }
        };
    }

    cachelib::init_cache_once(cache_config).unwrap();

    cachelib::init_cacheadmin("mononoke").unwrap();

    // Give each cache 5% of the available space, bar the blob cache which gets everything left
    // over. We can adjust this with data.
    let available_space = cachelib::get_available_space().unwrap();
    cachelib::get_or_create_pool(
        "blobstore-presence",
        get_usize(matches, "presence-cache-size", available_space / 20),
    )
    .unwrap();
    cachelib::get_or_create_pool(
        "changesets",
        get_usize(matches, "changesets-cache-size", available_space / 20),
    )
    .unwrap();
    cachelib::get_or_create_pool(
        "filenodes",
        get_usize(matches, "filenodes-cache-size", available_space / 20),
    )
    .unwrap();
    cachelib::get_or_create_pool(
        "bonsai_hg_mapping",
        get_usize(matches, "idmapping-cache-size", available_space / 20),
    )
    .unwrap();

    if matches.is_present("with-content-sha1-cache") {
        cachelib::get_or_create_pool(
            "content-sha1",
            get_usize(matches, "content-sha1-cache-size", available_space / 20),
        )
        .unwrap();
    }

    cachelib::get_or_create_pool(
        "blobstore-blobs",
        get_usize(
            matches,
            "blob-cache-size",
            cachelib::get_available_space().unwrap(),
        ),
    )
    .unwrap();
}

pub fn add_myrouter_args<'a, 'b>(app: App<'a, 'b>) -> App<'a, 'b> {
    app.args_from_usage(r"--myrouter-port=[PORT]    'port for local myrouter instance'")
}

pub fn read_configs<'a>(matches: &ArgMatches<'a>) -> Result<RepoConfigs> {
    let config_path = matches
        .value_of("mononoke-config-path")
        .ok_or(err_msg("mononoke-config-path must be specified"))?;
    RepoConfigs::read_configs(config_path)
}

pub fn get_config<'a>(matches: &ArgMatches<'a>) -> Result<(String, RepoConfig)> {
    let repo_id = get_repo_id(matches);

    let configs = read_configs(matches)?;
    let repo_config = configs
        .repos
        .into_iter()
        .filter(|(_, config)| RepositoryId::new(config.repoid) == repo_id)
        .last();
    match repo_config {
        Some((name, config)) => Ok((name, config)),
        None => Err(err_msg(format!("unknown repoid {:?}", repo_id))),
    }
}

fn open_repo_internal<'a>(
    logger: &Logger,
    matches: &ArgMatches<'a>,
    create: bool,
) -> impl Future<Item = BlobRepo, Error = Error> {
    let repo_id = get_repo_id(matches);

    let (reponame, config) = try_boxfuture!(get_config(matches));
    info!(logger, "using repo \"{}\" repoid {:?}", reponame, repo_id);
    let logger = match config.repotype {
        RepoType::BlobFiles(ref data_dir) => {
            setup_repo_dir(&data_dir, create).expect("Setting up file blobrepo failed");
            logger.new(o!["BlobRepo:Files" => data_dir.to_string_lossy().into_owned()])
        }
        RepoType::BlobRocks(ref data_dir) => {
            setup_repo_dir(&data_dir, create).expect("Setting up rocksdb blobrepo failed");
            logger.new(o!["BlobRepo:Rocksdb" => data_dir.to_string_lossy().into_owned()])
        }
        RepoType::BlobSqlite(ref data_dir) => {
            setup_repo_dir(&data_dir, create).expect("Setting up sqlite blobrepo failed");
            logger.new(o!["BlobRepo:Sqlite" => data_dir.to_string_lossy().into_owned()])
        }
        RepoType::BlobRemote {
            ref blobstores_args,
            ..
        } => logger.new(o!["BlobRepo:Remote" => format!("{:?}", blobstores_args)]),
    };

    let myrouter_port = parse_myrouter_port(matches);
    open_blobrepo(
        logger.clone(),
        config.repotype.clone(),
        repo_id,
        myrouter_port,
    )
    .boxify()
}

pub fn parse_blobstore_args<'a>(matches: &ArgMatches<'a>) -> RemoteBlobstoreArgs {
    // The unwraps here are safe because default values have already been provided in mononoke_app
    // above.
    match matches.value_of("mysql-blobstore-shardmap") {
        Some(shardmap) => RemoteBlobstoreArgs::Mysql(MysqlBlobstoreArgs {
            shardmap: shardmap.to_string(),
            shard_num: matches
                .value_of("mysql-blobstore-shard-num")
                .unwrap()
                .parse::<usize>()
                .ok()
                .and_then(NonZeroUsize::new)
                .expect("Provided mysql-blobstore-shard-num must be int larger than 0"),
        }),
        None => RemoteBlobstoreArgs::Manifold(ManifoldArgs {
            bucket: matches.value_of("manifold-bucket").unwrap().to_string(),
            prefix: matches.value_of("manifold-prefix").unwrap().to_string(),
        }),
    }
}

pub fn parse_myrouter_port<'a>(matches: &ArgMatches<'a>) -> Option<u16> {
    match matches.value_of("myrouter-port") {
        Some(port) => Some(
            port.parse::<u16>()
                .expect("Provided --myrouter-port is not u16"),
        ),
        None => None,
    }
}

pub fn parse_data_dir<'a>(matches: &ArgMatches<'a>) -> PathBuf {
    let data_dir = matches
        .value_of("data-dir")
        .expect("local data directory must be specified");
    Path::new(data_dir)
        .canonicalize()
        .expect("Failed to read local directory path")
}

pub fn get_usize_opt<'a>(matches: &ArgMatches<'a>, key: &str) -> Option<usize> {
    matches.value_of(key).map(|val| {
        val.parse::<usize>()
            .expect(&format!("{} must be integer", key))
    })
}

#[inline]
pub fn get_usize<'a>(matches: &ArgMatches<'a>, key: &str, default: usize) -> usize {
    get_usize_opt(matches, key).unwrap_or(default)
}

#[inline]
pub fn get_u64<'a>(matches: &ArgMatches<'a>, key: &str, default: u64) -> u64 {
    matches
        .value_of(key)
        .map(|val| {
            val.parse::<u64>()
                .expect(&format!("{} must be integer", key))
        })
        .unwrap_or(default)
}

pub fn get_i64_opt<'a>(matches: &ArgMatches<'a>, key: &str) -> Option<i64> {
    matches.value_of(key).map(|val| {
        val.parse::<i64>()
            .expect(&format!("{} must be integer", key))
    })
}
