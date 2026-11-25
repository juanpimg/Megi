//! Headless integration tests for the `shell` crate.
//!
//! These tests never create a winit `EventLoop` and never need a display server: they
//! exercise the pure CLI logic, the pixmap→framebuffer conversion and the headless
//! `--screenshot` path (the exact path CI uses).

mod basics;
mod common;
mod content;
mod find;
mod forms;
mod misc;
mod options;
mod pages;
mod rail_tests;
mod saves;
mod tabs;
mod worker;
