pub mod handlers;
pub mod models;
pub mod routes;
pub mod services;
pub mod validators;

#[cfg(test)]
mod tests;

pub use routes::messages_routes;
