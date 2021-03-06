// Copyright (c) 2018-present, Facebook, Inc.
// All Rights Reserved.
//
// This software may be used and distributed according to the terms of the
// GNU General Public License version 2 or any later version.

//! Scaffolding that's generally useful to build CLI tools on top of Mononoke.

#![deny(warnings)]
#![feature(never_type)]

extern crate ascii;
extern crate bytes;
extern crate cachelib;
extern crate clap;
#[macro_use]
extern crate failure_ext as failure;
extern crate futures;
#[macro_use]
extern crate futures_ext;
#[macro_use]
extern crate slog;
extern crate sloggers;
extern crate tokio;
extern crate tracing;
extern crate upload_trace;
extern crate uuid;

extern crate slog_glog_fmt;

extern crate blobrepo;
extern crate blobrepo_factory;
extern crate bookmarks;
extern crate changesets;
extern crate context;
extern crate mercurial;
extern crate mercurial_types;
extern crate metaconfig_parser;
extern crate metaconfig_types;
extern crate mononoke_types;
extern crate panichandler;
extern crate scuba_ext;
extern crate sshrelay;

pub mod args;
