// Copyright (c) 2018-present, Facebook, Inc.
// All Rights Reserved.
//
// This software may be used and distributed according to the terms of the
// GNU General Public License version 2 or any later version.

#[derive(Debug, Fail)]
pub enum ErrorKind {
    #[fail(display = "Blob {} not found in blobstore", _0)] NotFound(String),
}
