#![allow(clippy::result_large_err)] // tonic::Status is ~176 bytes; pervasive in this crate

pub mod admin;
pub mod auth;
pub mod config;
pub mod conv;
pub mod error;
pub mod handler;
pub mod pb;
pub mod ratelimit;
pub mod rest;
pub mod validation;
