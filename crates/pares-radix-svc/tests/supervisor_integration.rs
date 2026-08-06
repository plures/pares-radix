//! Integration tests for [`PluginSupervisor`] — real process spawn, real
//! privilege-grant enforcement, real health check. Uses the trivial fixture
//! binaries (`plugin-fixture-long-lived`, `plugin-fixture-exit-immediately`)
//! as stand-ins for the full `pares-agens` binary, per the FIX-2 test
//! requirement: "spawn a trivial test binary as the child, confirm privilege
//! grant flows through, confirm an unlisted fake plugin name is rejected
//! end-to-end (not just unit-level)."

use std::sync::Arc;
use std::time::Duration;

use pares_radix_core::spine::plugin_privilege_actions::{
    PluginPrivilegeActionHandler, ALLOWLIST_KEY,
};
use pares_radix_core::state::InMemoryStateStore;
use pares_radix_core::StateStore;
use pares_radix_svc::supervisor::{PluginSpawnRequest, PluginSupervisor, SupervisorError};

fn fixture_program() -> String {
    env!("CARGO_BIN_EXE_plugin-fixture-long-lived").to_string()
}

fn fixture_exit_program() -> String {
    env!("CARGO_BIN_EXE_plugin-fixture-exit-immediately").to_string()
}

fn fixture_echo_argv_program() -> String {
    env!("CARGO_BIN_EXE_plugin-fixture-echo-argv").to_string()
}

fn supervisor() -> PluginSupervisor {
    let handler = Arc::new(PluginPrivilegeActionHandler::new(Arc::new(
        InMemoryStateStore::new(),
    )));
    PluginSupervisor::new(handler).with_health_check_delay(Duration::from_millis(50))
}

#[tokio::test]
async fn allowlisted_plugin_spawns_a_real_process_and_is_alive() {
    let sup = supervisor();
    let mut plugin = sup
        .spawn(PluginSpawnRequest {
            plugin_id: "pares-agens".to_string(),
            channel_id: "test-channel-alive".to_string(),
            program: fixture_program(),
            args: vec![],
            envs: vec![],
        })
        .await
        .expect("allowlisted plugin should spawn");
    assert!(plugin.is_alive().await, "child should still be running");
    assert!(
        plugin.pid().is_some(),
        "expected a real OS process id for the supervised child"
    );
    assert!(
        !plugin.decision_id.is_empty(),
        "expected a real model-invocation grant decision id"
    );
    plugin.shutdown().await.expect("shutdown should succeed");
}

#[tokio::test]
async fn unlisted_fake_plugin_is_rejected_end_to_end_no_process_ever_starts() {
    let sup = supervisor();
    let result = sup
        .spawn(PluginSpawnRequest {
            plugin_id: "totally-fake-plugin".to_string(),
            channel_id: "test-channel-fake".to_string(),
            program: fixture_program(),
            args: vec![],
            envs: vec![],
        })
        .await;
    assert!(
        matches!(result, Err(SupervisorError::NotPrivileged(_))),
        "expected NotPrivileged, got {result:?}",
        result = result.map(|_| "Ok(spawned!)").err()
    );
}

#[tokio::test]
async fn two_privileged_plugins_cannot_own_the_same_channel_end_to_end() {
    let state = InMemoryStateStore::new();
    state
        .set(
            ALLOWLIST_KEY,
            serde_json::json!(["pares-agens", "second-special-plugin"]),
        )
        .await;
    let handler = Arc::new(PluginPrivilegeActionHandler::new(Arc::new(state)));
    let sup = PluginSupervisor::new(Arc::clone(&handler))
        .with_health_check_delay(Duration::from_millis(50));

    let mut first = sup
        .spawn(PluginSpawnRequest {
            plugin_id: "pares-agens".to_string(),
            channel_id: "shared-channel".to_string(),
            program: fixture_program(),
            args: vec![],
            envs: vec![],
        })
        .await
        .expect("first plugin should spawn and own the channel");
    assert!(first.is_alive().await);

    let second = sup
        .spawn(PluginSpawnRequest {
            plugin_id: "second-special-plugin".to_string(),
            channel_id: "shared-channel".to_string(),
            program: fixture_program(),
            args: vec![],
            envs: vec![],
        })
        .await;
    assert!(matches!(
        second,
        Err(SupervisorError::ChannelGrantDenied(_))
    ));

    first.shutdown().await.expect("shutdown should succeed");
}

#[tokio::test]
async fn plugin_child_that_crashes_immediately_fails_health_check() {
    let sup = supervisor();
    let result = sup
        .spawn(PluginSpawnRequest {
            plugin_id: "pares-agens".to_string(),
            channel_id: "test-channel-crash".to_string(),
            program: fixture_exit_program(),
            args: vec![],
            envs: vec![],
        })
        .await;
    assert!(matches!(
        result,
        Err(SupervisorError::HealthCheckFailed(_))
    ));
}

/// FIX-5: proves the supervisor actually passes real `args`/`envs` through
/// to the spawned child process (not just that `PluginSpawnRequest` was
/// constructed with them) by spawning a fixture binary that writes its
/// received argv and a marker env var to a file, then asserting the file's
/// contents match what was requested.
#[tokio::test]
async fn spawned_child_actually_receives_real_args_and_envs() {
    let sup = supervisor();
    let out_dir = std::env::temp_dir().join(format!(
        "radix-svc-echo-argv-test-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&out_dir).expect("create temp dir");
    let out_file = out_dir.join("argv.txt");

    let mut plugin = sup
        .spawn(PluginSpawnRequest {
            plugin_id: "pares-agens".to_string(),
            channel_id: "test-channel-echo-argv".to_string(),
            program: fixture_echo_argv_program(),
            args: vec![
                "serve".to_string(),
                "--copilot".to_string(),
                "--model".to_string(),
                "gpt-4.1".to_string(),
                "--deep-model".to_string(),
                "gpt-5.2".to_string(),
            ],
            envs: vec![
                (
                    "PLUGIN_FIXTURE_ECHO_ARGV_OUT".to_string(),
                    out_file.to_string_lossy().to_string(),
                ),
                (
                    "PLUGIN_FIXTURE_ECHO_ENV_MARKER".to_string(),
                    "real-env-value-not-a-stub".to_string(),
                ),
            ],
        })
        .await
        .expect("echo-argv fixture should spawn");
    assert!(plugin.is_alive().await, "child should still be running");

    // The fixture writes the file on startup, before its long-lived sleep
    // loop; give it a brief window to flush to disk.
    tokio::time::sleep(Duration::from_millis(100)).await;
    let contents = std::fs::read_to_string(&out_file).expect("fixture should have written argv file");
    let lines: Vec<&str> = contents.lines().collect();
    assert_eq!(
        lines,
        vec![
            "serve",
            "--copilot",
            "--model",
            "gpt-4.1",
            "--deep-model",
            "gpt-5.2",
            "ENV_MARKER=real-env-value-not-a-stub",
        ],
        "spawned child did not receive the real args/envs from PluginSpawnRequest"
    );

    plugin.shutdown().await.expect("shutdown should succeed");
    let _ = std::fs::remove_dir_all(&out_dir);
}
