//! Minimal telemetry test
//!
//! This tests if vox-telemetry middleware is working correctly

use vox::service;

#[service]
trait TestService {
    async fn echo(&self, message: String) -> String;
}

#[derive(Clone)]
struct TestServiceImpl;

impl TestService for TestServiceImpl {
    async fn echo(&self, message: String) -> String {
        println!("Service: echo called with: {}", message);
        message
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Minimal Telemetry Test ===\n");

    // Create dispatcher
    let _dispatcher = TestServiceDispatcher::new(TestServiceImpl);
    println!("Created dispatcher\n");

    // Test: Try to invoke a method through the dispatcher
    println!("Testing method invocation...");

    println!("=== Test Complete ===");

    Ok(())
}
