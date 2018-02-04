// Copyright 2014 The Rust Project Developers.
// Copyright 2017 Seiichi Uchida <uchida@os.ecc.u-tokyo.ac.jp>.

//! # The MSC Lock Library
//!
//! The MSC Lock Library provides an implementation of mutual exclution based on
//! MSC lock algorithm. Unlike traditional one, the API of this library does not
//! require an explicit arguement to a pointer of queue node. Therefore, it can
//! be interchangebely used with `std::sync::Mutex`.
//!
//! Currently most of the documents and examples are borrowed from those of `std::sync::Mutex`.

#![crate_type = "lib"]
#![feature(optin_builtin_traits)]
#![feature(const_fn)]

pub use mcs::*;
pub mod mcs;

mod poison;
