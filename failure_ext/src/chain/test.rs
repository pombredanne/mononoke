// Copyright 2004-present Facebook. All Rights Reserved.

use super::*;

#[derive(Fail, Debug)]
#[fail(display = "RootError badness")]
struct RootError;
#[derive(Fail, Debug)]
#[fail(display = "Bar badness")]
struct Bar;
#[derive(Fail, Debug)]
#[fail(display = "Blat badness")]
struct Blat;

#[test]
fn simple() {
    let err = Chain::with_fail(Bar, RootError);

    assert_eq!(format!("{}", err), "Bar badness");
}

#[test]
fn simple_result() {
    let res: Result<(), _> = Err(RootError);
    let res2 = res.chain_err(Bar);

    assert!(res2.is_err());
    assert_eq!(
        format!("{:?}", res2),
        "Err(Chain { err: Bar, cause: Fail(RootError) })"
    )
}

#[test]
fn simple_causes() {
    let err = RootError.chain_err(Bar);

    assert_eq!(format!("{}", err), "Bar badness");

    assert_eq!(
        format!("{:#}", err),
        "Bar badness\n\
         \x20 caused by: RootError badness"
    );
}

#[test]
fn long_causes() {
    let err = RootError.chain_err(Bar).chain_err(Blat);

    assert_eq!(format!("{}", err), "Blat badness");

    assert_eq!(
        format!("{:#}", err),
        "Blat badness\n\
         \x20 caused by: Bar badness\n\
         \x20 caused by: RootError badness"
    );
}

#[test]
fn simple_error() {
    let err = Error::from(RootError).chain_err(Bar);

    assert_eq!(format!("{}", err), "Bar badness");
}

#[test]
fn simple_error_result() {
    let res: Result<(), _> = Err(Error::from(RootError));
    let res2 = res.chain_err(Bar);

    assert!(res2.is_err());
    assert_eq!(
        format!("{:?}", res2),
        "Err(Chain { err: Bar, cause: Error(RootError) })"
    )
}

#[test]
fn simple_causes_error() {
    let err = Error::from(RootError).chain_err(Bar);

    assert_eq!(format!("{}", err), "Bar badness");

    assert_eq!(
        format!("{:#}", err),
        "Bar badness\n\
         \x20 caused by: RootError badness"
    );
}

#[test]
fn long_causes_error() {
    let err = Error::from(RootError).chain_err(Bar).chain_err(Blat);

    assert_eq!(format!("{}", err), "Blat badness");

    assert_eq!(
        format!("{:#}", err),
        "Blat badness\n\
         \x20 caused by: Bar badness\n\
         \x20 caused by: RootError badness"
    );
}
