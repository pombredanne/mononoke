// Copyright (c) 2018-present, Facebook, Inc.
// All Rights Reserved.
//
// This software may be used and distributed according to the terms of the
// GNU General Public License version 2 or any later version.

include "scm/mononoke/mononoke_types/if/mononoke_types_thrift.thrift"

# Memcache constants. Should be change when we want to invalidate memcache
# entries
const i32 MC_CODEVER = 0
const i32 MC_SITEVER = 0

typedef i32 RepoId (hs.newtype)

 // Thrift does not support unsigned, so using i64 here
typedef i64 GenerationNum (hs.newtype)

struct ChangesetEntry {
  1: required RepoId repo_id,
  2: required mononoke_types_thrift.ChangesetId cs_id,
  3: required list<mononoke_types_thrift.ChangesetId> parents,
  4: required GenerationNum gen,
}
