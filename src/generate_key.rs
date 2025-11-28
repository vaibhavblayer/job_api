// generate_key.rs
// Utility to generate a new encryption key for the system

mod encryption;

use encryption::EncryptionService;

fn main() {
    println!("Generating new AES-256 encryption key...\n");

    let key = EncryptionService::generate_key();

    println!("✅ Key generated successfully!\n");
    println!("Add this to your .env file:");
    println!("─────────────────────────────────────────────────");
    println!("ENCRYPTION_MASTER_KEY={}", key);
    println!("─────────────────────────────────────────────────");
    println!("\n⚠️  IMPORTANT:");
    println!("  • Keep this key secure and never commit it to version control");
    println!("  • Store a backup in a secure location");
    println!("  • If you lose this key, encrypted data cannot be recovered");
    println!("\nTo test the key:");
    println!("  cargo run --bin test_encryption");
}
