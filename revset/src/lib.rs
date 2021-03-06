// Copyright (c) 2017-present, Facebook, Inc.
// All Rights Reserved.
//
// This software may be used and distributed according to the terms of the
// GNU General Public License version 2 or any later version.

#![deny(warnings)]

#[cfg(test)]
extern crate blobrepo;
extern crate changeset_fetcher;
extern crate changesets;
#[macro_use]
extern crate cloned;
extern crate context;
#[macro_use]
extern crate failure_ext as failure;
#[macro_use]
extern crate futures;
extern crate futures_ext;
#[macro_use]
extern crate maplit;
extern crate mercurial_types;
extern crate mononoke_types;
#[cfg(test)]
extern crate mononoke_types_mocks;
extern crate reachabilityindex;
#[cfg(test)]
extern crate revset_test_helper;
#[cfg(test)]
extern crate skiplist;
extern crate uniqueheap;

use futures::stream::Stream;
use mercurial_types::HgNodeHash;
use mononoke_types::ChangesetId;

mod setcommon;

mod intersectnodestream;
pub use intersectnodestream::IntersectNodeStream;

mod unionnodestream;
pub use unionnodestream::UnionNodeStream;

mod setdifferencenodestream;
pub use setdifferencenodestream::SetDifferenceNodeStream;

pub mod errors;
pub use errors::{Error, ErrorKind};

pub type NodeStream = Stream<Item = HgNodeHash, Error = errors::Error> + Send + 'static;
pub type BonsaiNodeStream = Stream<Item = ChangesetId, Error = errors::Error> + Send + 'static;

mod validation;
pub use validation::ValidateNodeStream;

mod ancestors;
pub use ancestors::{common_ancestors, greatest_common_ancestor, AncestorsNodeStream};

mod ancestorscombinators;
pub use ancestorscombinators::DifferenceOfUnionsOfAncestorsNodeStream;

mod range;
pub use range::RangeNodeStream;

use uniqueheap::UniqueHeap;

pub use test::*;

#[cfg(test)]
mod test {
    pub extern crate ascii;
    pub extern crate async_unit;
    pub extern crate quickcheck;

    pub extern crate fixtures;
}
#[cfg(not(test))]
mod test {}
#[cfg(test)]
mod quickchecks;
#[cfg(test)]
mod tests;
