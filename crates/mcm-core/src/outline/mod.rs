//! The outline language: user input syntax and native file body in one grammar.

pub mod lexer;
pub mod parser;
pub mod recover;
pub mod serialize;

pub use lexer::{Token, escape_title, lex, tokenize_body};
pub use parser::{ParseOutput, parse};
pub use recover::{RecoveryReport, is_binary, normalise_input, report_for};
pub use serialize::serialize;
