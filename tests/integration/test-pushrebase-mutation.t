  $ . $TESTDIR/library.sh

setup configuration
  $ setup_common_config "blob:files"
  $ cd $TESTTMP

setup common configuration
  $ setconfig ui.ssh="\"$DUMMYSSH\"" mutation.date="0 0"
  $ enable amend

  $ newrepo repo-hg
  $ setup_hg_server
  $ echo base > base
  $ hg commit -Aqm base
  $ hg bookmark master -r tip

blobimport
  $ cd $TESTTMP
  $ blobimport repo-hg/.hg repo

start mononoke
  $ mononoke
  $ wait_for_mononoke $TESTTMP/repo

clone the repo
  $ hgclone_treemanifest ssh://user@dummy/repo-hg client --noupdate --config extensions.remotenames= -q
  $ cd client
  $ setup_hg_client
  $ enable pushrebase remotenames

create a commit with mutation extras
  $ hg up -q 0
  $ echo 1 > 1 && hg add 1 && hg commit -m 1
  $ echo 1a > 1 && hg amend -m 1a --config mutation.record=true
  $ hg debugmutation .
    62fddca11dc5682b0377a89881b0fff7c042b7c4 amend by test at 1970-01-01T00:00:00 from:
      f0161ad23099c690115006c21e96f780f5d740b6

pushrebase it directly onto master - it will be rewritten without the mutation extras
  $ hgmn push -r . --to master
  remote: * Session with Mononoke started with uuid: * (glob)
  pushing rev 62fddca11dc5 to destination ssh://user@dummy/repo bookmark master
  searching for changes
  adding changesets
  adding manifests
  adding file changes
  added 1 changesets with 0 changes to 0 files (+1 heads)
  updating bookmark master

  $ tglog
  o  3: a05b3505b7d1 '1a'
  |
  | @  2: 62fddca11dc5 '1a'
  |/
  o  0: d20a80d4def3 'base'
  
  $ hg debugmutation master
    a05b3505b7d1aac5fd90b09a5f014822647ec205

create another commit on the base commit with mutation extras
  $ hg up 0
  0 files updated, 0 files merged, 1 files removed, 0 files unresolved
  $ echo 2 > 2 && hg add 2 && hg commit -m 2
  $ echo 2a > 2 && hg amend -m 2a --config mutation.record=true
  $ hg debugmutation .
    a805bbad06476c09727930617acfe8d6623f8d88 amend by test at 1970-01-01T00:00:00 from:
      1b9fe529321657f93e84f23afaf9c855b9af34ff

pushrebase it onto master - it will be rebased and rewritten without the mutation extras
  $ hgmn push -r . --to master
  remote: * Session with Mononoke started with uuid: * (glob)
  pushing rev a805bbad0647 to destination ssh://user@dummy/repo bookmark master
  searching for changes
  adding changesets
  adding manifests
  adding file changes
  added 1 changesets with 0 changes to 0 files
  updating bookmark master

  $ tglog
  o  6: 7042a534cddc '2a'
  |
  | @  5: a805bbad0647 '2a'
  | |
  o |  3: a05b3505b7d1 '1a'
  |/
  | o  2: 62fddca11dc5 '1a'
  |/
  o  0: d20a80d4def3 'base'
  
  $ hg debugmutation master
    7042a534cddcd761aeea38446ce39590634568e8
