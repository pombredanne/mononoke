  $ . $TESTDIR/library.sh

setup configuration
  $ setup_common_config "blob:files"
  $ cd $TESTTMP

setup common configuration
  $ cat >> $HGRCPATH <<EOF
  > [ui]
  > ssh="$DUMMYSSH"
  > [extensions]
  > amend=
  > EOF

Setup helpers
  $ log() {
  >   hg sl -T "{desc} [{phase};rev={rev};{node|short}] {remotenames}" "$@"
  > }

setup repo
  $ hg init repo-hg
  $ cd repo-hg
  $ setup_hg_server
  $ hg debugdrawdag <<EOF
  > C
  > |
  > B
  > |
  > A
  > EOF

create master bookmark

  $ hg bookmark master_bookmark -r tip

blobimport them into Mononoke storage and start Mononoke
  $ cd ..
  $ blobimport repo-hg/.hg repo

start mononoke
  $ mononoke
  $ wait_for_mononoke $TESTTMP/repo

Clone the repo, do not set up pushrebase
  $ hgclone_treemanifest ssh://user@dummy/repo-hg repo2 --noupdate --config extensions.remotenames= -q
  $ cd repo2
  $ setup_hg_client
  $ cat >> .hg/hgrc <<EOF
  > [extensions]
  > remotenames =
  > EOF

Create a new commit in repo2
  $ hg up -q 2 && echo 1 > 1 && hg add 1 && hg ci -qm 1

Do a push, while bundle preservation is disabled
  $ hgmn push -qr . --to master_bookmark
  $ ls $TESTTMP/repo/blobs | grep rawbundle2
  [1]

Restart mononoke with enabled bundle2 preservation
  $ kill $MONONOKE_PID
  $ rm -rf $TESTTMP/mononoke-config
  $ export ENABLE_PRESERVE_BUNDLE2=1
  $ setup_common_config "blob:files"
  $ mononoke
  $ wait_for_mononoke $TESTTMP/repo

Create a new commit in repo2
  $ cd $TESTTMP/repo2
  $ echo 2 > 2 && hg add 2 && hg ci -qm 2

Do a push, while bundle preservation is enabled
  $ hgmn push -r . --to master_bookmark
  remote: .* DEBG Session with Mononoke started with uuid: .* (re)
  pushing rev dc31470c8386 to destination ssh://user@dummy/repo bookmark master_bookmark
  searching for changes
  updating bookmark master_bookmark

  $ ls $TESTTMP/repo/blobs | grep rawbundle2
  blob-repo0000.rawbundle2.blake2.f549cc8c5041352d3e9cc84bd37027836dc8ec5323fbd63c17b4b4d6f2223262

Do a pushrebase, while preservation is enabled
  $ hg up -q 2 && echo 3 > 3 && hg add 3 && hg ci -qm 3
  $ hgmn push -r . --to master_bookmark --config extensions.pushrebase=
  remote: .* DEBG Session with Mononoke started with uuid: .* (re)
  pushing rev 1c1c6e358bc0 to destination ssh://user@dummy/repo bookmark master_bookmark
  searching for changes
  adding changesets
  adding manifests
  adding file changes
  added 1 changesets with 0 changes to 0 files
  updating bookmark master_bookmark
  $ ls $TESTTMP/repo/blobs | grep rawbundle2
  blob-repo0000.rawbundle2.blake2.46be48908b04cb2395751ab5d75fb82cd7b5bc9ea21003326db6b413e5311029
  blob-repo0000.rawbundle2.blake2.f549cc8c5041352d3e9cc84bd37027836dc8ec5323fbd63c17b4b4d6f2223262