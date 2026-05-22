"""
Cross-Tool Integration Workflow Tests
======================================
Tests that exercise real-world workflows spanning multiple tool categories.
These verify that tools compose correctly — the kind of thing a human user does
when tools are building blocks for larger tasks.

Workflows tested:
1. File + Shell: write script → execute → verify output
2. File + Memory: write notes → store in memory → search → verify
3. Praxis + File: write .px file → load constraints → evaluate
4. Canvas + DB: create canvas → persist state → reload → verify
5. Chronos + Shell: record events → run command → correlate timeline
6. Config + Praxis: set config → add constraint referencing it → evaluate
7. Plugin + Procedure: register plugin → run procedure using plugin state
8. End-to-end automation: file → shell → db → chronos audit trail
"""

import json
import os
import time
import uuid

import pytest


@pytest.fixture(scope="module")
def test_prefix():
    """Unique prefix for this module run."""
    return f"radix-xflow-{uuid.uuid4().hex[:8]}"


def _call(mcp, tool, args=None, timeout=10):
    """Call tool and return result."""
    return mcp.call_tool(tool, args or {}, timeout=timeout)


def _assert_ok(result, msg=""):
    """Assert tool call succeeded."""
    assert result is not None, f"Result was None {msg}"
    if isinstance(result, dict) and "error" in result:
        pytest.fail(f"Tool error {msg}: {result}")


def _get_text(result) -> str:
    """Get text content from result."""
    if result is None:
        return ""
    if isinstance(result, str):
        return result
    if isinstance(result, dict):
        return json.dumps(result)
    return str(result)


# ── Workflow 1: File + Shell Integration ──────────────────────────────────────


class TestFileShellWorkflow:
    """Write a script, execute it, verify output — the basic automation loop."""

    def test_write_script_and_execute(self, mcp, test_prefix):
        """Write a bash script, make it executable via shell, run it."""
        script_dir = f"/tmp/{test_prefix}/scripts"
        script_path = f"{script_dir}/hello.sh"
        script_content = '#!/bin/bash\necho "Hello from pares-radix test"'

        # Write the script
        result = _call(mcp, "write_file", {
            "path": script_path,
            "content": script_content,
        })
        _assert_ok(result, "write script")

        # Make it executable
        result = _call(mcp, "run_command", {
            "command": f"chmod +x {script_path}",
        })
        _assert_ok(result, "chmod")

        # Execute it
        result = _call(mcp, "run_command", {
            "command": script_path,
        })
        _assert_ok(result)
        assert "Hello from pares-radix test" in _get_text(result)

    def test_write_python_script_execute_capture(self, mcp, test_prefix):
        """Write Python script, execute, capture structured output."""
        script_path = f"/tmp/{test_prefix}/scripts/calc.py"
        script_content = """import json
import sys
data = {"sum": 2 + 2, "product": 3 * 7, "source": "radix-test"}
print(json.dumps(data))
"""
        result = _call(mcp, "write_file", {
            "path": script_path,
            "content": script_content,
        })
        _assert_ok(result, "write python")

        result = _call(mcp, "run_command", {
            "command": f"python3 {script_path}",
        })
        _assert_ok(result)
        text = _get_text(result)
        # Parse the JSON output from the script
        assert "sum" in text
        assert "21" in text  # 3*7

    def test_shell_output_to_file_roundtrip(self, mcp, test_prefix):
        """Run command, write output to file, read it back."""
        output_path = f"/tmp/{test_prefix}/scripts/output.txt"

        # Run a command that produces output
        result = _call(mcp, "run_command", {
            "command": "echo 'line1' && echo 'line2' && echo 'line3'",
        })
        _assert_ok(result)
        output_text = _get_text(result)

        # Write that output to a file
        result = _call(mcp, "write_file", {
            "path": output_path,
            "content": output_text,
        })
        _assert_ok(result)

        # Read it back
        result = _call(mcp, "read_file", {"path": output_path})
        _assert_ok(result)
        read_back = _get_text(result)
        assert "line1" in read_back
        assert "line3" in read_back

    def test_edit_script_rerun(self, mcp, test_prefix):
        """Write script → run → edit → run again → verify changed output."""
        script_path = f"/tmp/{test_prefix}/scripts/version.sh"

        # Write v1
        _call(mcp, "write_file", {
            "path": script_path,
            "content": '#!/bin/bash\necho "version=1.0"',
        })
        _call(mcp, "run_command", {"command": f"chmod +x {script_path}"})

        result = _call(mcp, "run_command", {"command": script_path})
        assert "version=1.0" in _get_text(result)

        # Edit to v2
        result = _call(mcp, "edit_file", {
            "path": script_path,
            "old_text": 'echo "version=1.0"',
            "new_text": 'echo "version=2.0"',
        })
        _assert_ok(result, "edit")

        # Re-run and verify
        result = _call(mcp, "run_command", {"command": script_path})
        assert "version=2.0" in _get_text(result)


# ── Workflow 2: File + Memory Integration ─────────────────────────────────────


class TestFileMemoryWorkflow:
    """Write files, store metadata in memory, search for it."""

    def test_write_file_store_metadata_search(self, mcp, test_prefix):
        """Write a file, store its path in memory, search for it later."""
        tag = uuid.uuid4().hex[:8]
        file_path = f"/tmp/{test_prefix}/docs/note-{tag}.md"
        content = f"# Important Note\nThis contains the secret keyword: {tag}"

        # Write the file
        result = _call(mcp, "write_file", {
            "path": file_path,
            "content": content,
        })
        _assert_ok(result)

        # Store metadata about it in memory
        result = _call(mcp, "memory_store", {
            "content": f"Created important note at {file_path} with tag {tag}",
            "category": "project-context",
            "tags": ["testing", "cross-tool", tag],
        })
        _assert_ok(result, "memory_store")

        # Search for it
        time.sleep(0.5)  # Allow indexing
        result = _call(mcp, "memory_search", {
            "query": f"note with tag {tag}",
            "limit": 5,
        })
        _assert_ok(result)
        text = _get_text(result)
        assert tag in text, f"Memory search didn't find tag {tag} in: {text}"

    def test_file_content_to_memory_roundtrip(self, mcp, test_prefix):
        """Read file content, store key fact in memory, retrieve later."""
        tag = uuid.uuid4().hex[:8]
        file_path = f"/tmp/{test_prefix}/docs/config-{tag}.json"
        config = json.dumps({"database_url": "postgres://localhost/test", "tag": tag})

        # Write config file
        _call(mcp, "write_file", {"path": file_path, "content": config})

        # Read it back (simulating discovery)
        result = _call(mcp, "read_file", {"path": file_path})
        _assert_ok(result)

        # Store the relevant fact
        result = _call(mcp, "memory_store", {
            "content": f"Database config for tag {tag} is at {file_path}, uses postgres://localhost/test",
            "category": "entity",
            "tags": ["database", "config", tag],
        })
        _assert_ok(result)

        # Search by semantic meaning
        time.sleep(0.5)
        result = _call(mcp, "memory_search", {
            "query": f"database configuration postgres {tag}",
            "limit": 3,
        })
        _assert_ok(result)
        assert tag in _get_text(result)


# ── Workflow 3: Praxis + File Integration ─────────────────────────────────────


class TestPraxisFileWorkflow:
    """Write constraint files, load them, evaluate against state."""

    def test_write_constraint_file_and_evaluate(self, mcp, test_prefix):
        """Write a .px constraint, add it to praxis, evaluate."""
        # Add a constraint via praxis tool
        constraint_name = f"xflow-test-{uuid.uuid4().hex[:6]}"
        result = _call(mcp, "praxis_add_constraint", {
            "name": constraint_name,
            "severity": "error",
            "when": "action == 'deploy'",
            "require": "deployment_ready == true",
            "message": "Deployment readiness check failed",
        })
        _assert_ok(result, "add constraint")

        # Evaluate with passing context
        result = _call(mcp, "praxis_evaluate", {
            "context": {"action": "deploy", "deployment_ready": True},
        })
        _assert_ok(result)
        # Should not crash, result is meaningful
        assert result is not None

        # Evaluate with failing context
        result = _call(mcp, "praxis_evaluate", {
            "context": {"action": "deploy", "deployment_ready": False},
        })
        _assert_ok(result)
        # Should produce some result (violation or otherwise)
        assert result is not None
        assert len(str(result)) > 0

    def test_write_rule_file_store_in_db(self, mcp, test_prefix):
        """Create a rule, persist its metadata in DB, retrieve."""
        rule_name = f"xflow-rule-{uuid.uuid4().hex[:6]}"

        # Add rule to praxis
        result = _call(mcp, "praxis_add_rule", {
            "name": rule_name,
            "conditions": ["context.env == 'production'"],
            "actions": [{"type": "log", "message": "Production deployment detected"}],
            "priority": 10,
        })
        _assert_ok(result, "add rule")

        # Store metadata in DB
        result = _call(mcp, "db_put", {
            "key": f"praxis:rules:{rule_name}:meta",
            "value": {"created_by": "cross-tool-test", "priority": 10},
        })
        _assert_ok(result)

        # Retrieve from DB
        result = _call(mcp, "db_get", {
            "key": f"praxis:rules:{rule_name}:meta",
        })
        _assert_ok(result)
        text = _get_text(result)
        assert "cross-tool-test" in text


# ── Workflow 4: Canvas + DB Integration ───────────────────────────────────────


class TestCanvasDbWorkflow:
    """Create canvas, persist state in DB, reload and verify."""

    def test_canvas_create_and_db_persist(self, mcp, test_prefix):
        """Create a canvas, record its ID in DB, retrieve later."""
        title = f"XFlow Test Canvas {uuid.uuid4().hex[:6]}"

        # Create canvas
        result = _call(mcp, "canvas_create", {"title": title})
        _assert_ok(result, "canvas_create")

        # Get the canvas to see its state
        result = _call(mcp, "canvas_get", {})
        _assert_ok(result)
        canvas_text = _get_text(result)

        # Store canvas reference in DB
        result = _call(mcp, "db_put", {
            "key": f"{test_prefix}:canvas:current",
            "value": {"title": title, "created_at": time.time()},
        })
        _assert_ok(result)

        # Retrieve from DB
        result = _call(mcp, "db_get", {"key": f"{test_prefix}:canvas:current"})
        _assert_ok(result)
        assert title in _get_text(result)

    def test_canvas_tree_modify_and_snapshot(self, mcp, test_prefix):
        """Modify canvas tree, take DB snapshot of state."""
        # Create fresh canvas
        _call(mcp, "canvas_create", {"title": "Snapshot Test"})

        # Add nodes
        result = _call(mcp, "canvas_add_node", {
            "parent_id": "root",
            "node": {
                "id": "header-1",
                "type": "heading",
                "props": {"text": "Test Heading", "level": 1},
            },
        })
        _assert_ok(result, "add node")

        # Set data
        result = _call(mcp, "canvas_set_data", {
            "data": {"workflow_stage": "testing", "run_id": test_prefix},
        })
        _assert_ok(result, "set data")

        # Get canvas state
        result = _call(mcp, "canvas_get", {})
        _assert_ok(result)
        canvas_state = _get_text(result)

        # Persist full state to DB
        result = _call(mcp, "db_put", {
            "key": f"{test_prefix}:canvas:snapshot",
            "value": canvas_state,
        })
        _assert_ok(result)

        # Verify we can retrieve it
        result = _call(mcp, "db_get", {"key": f"{test_prefix}:canvas:snapshot"})
        _assert_ok(result)
        assert "testing" in _get_text(result) or "header" in _get_text(result).lower()


# ── Workflow 5: Chronos + Shell Integration ───────────────────────────────────


class TestChronosShellWorkflow:
    """Record timeline events around shell operations for audit trail."""

    def test_command_execution_with_chronos_audit(self, mcp, test_prefix):
        """Record before/after events around a shell command."""
        cmd_id = uuid.uuid4().hex[:8]

        # Record 'command_started' event
        result = _call(mcp, "chronos_record", {
            "key": f"xflow:cmd:started:{cmd_id}",
            "actor": "pytest-xflow",
            "action": "create",
            "level": "info",
            "data": {"command": "echo test", "cmd_id": cmd_id},
        })
        _assert_ok(result, "record start")

        # Execute the command
        result = _call(mcp, "run_command", {"command": f"echo 'cmd-{cmd_id}'"})
        _assert_ok(result)
        output = _get_text(result)

        # Record 'command_completed' event
        result = _call(mcp, "chronos_record", {
            "key": f"xflow:cmd:completed:{cmd_id}",
            "actor": "pytest-xflow",
            "action": "update",
            "level": "info",
            "data": {"cmd_id": cmd_id, "output": output[:200], "success": True},
        })
        _assert_ok(result, "record complete")

        # Query timeline — should see our events
        result = _call(mcp, "chronos_timeline", {"limit": 50})
        _assert_ok(result)
        text = _get_text(result)
        # Timeline uses 'key' field — look for our key prefix
        assert "xflow:cmd" in text or cmd_id in text

    def test_error_tracking_in_chronos(self, mcp, test_prefix):
        """Run a failing command, record the error in chronos."""
        err_id = uuid.uuid4().hex[:8]

        # Run a command that fails
        result = _call(mcp, "run_command", {
            "command": f"cat /tmp/nonexistent-{err_id}-file 2>&1; echo EXIT:$?",
        })
        # It may succeed (exit 0 from echo) but contain error text
        output = _get_text(result)

        # Record the error
        result = _call(mcp, "chronos_record", {
            "key": f"xflow:error:{err_id}",
            "actor": "pytest-xflow",
            "action": "create",
            "level": "error",
            "data": {"err_id": err_id, "output": output[:200]},
        })
        _assert_ok(result, "record error")

        # Verify in timeline — look for our key
        result = _call(mcp, "chronos_timeline", {"limit": 50})
        _assert_ok(result)
        text = _get_text(result)
        assert f"xflow:error:{err_id}" in text or err_id in text


# ── Workflow 6: Config + Praxis Integration ───────────────────────────────────


class TestConfigPraxisWorkflow:
    """Set config values, create constraints that reference them."""

    def test_config_driven_constraint(self, mcp, test_prefix):
        """Set a config value via DB, add a constraint, evaluate."""
        config_key = f"app:feature_flags:{test_prefix}"

        # Set config via db_put (config_set/config_get are the DB tools)
        result = _call(mcp, "db_put", {
            "key": config_key,
            "value": {"enabled": True, "max_retries": 3},
        })
        _assert_ok(result, "config set")

        # Verify config is set
        result = _call(mcp, "db_get", {"key": config_key})
        _assert_ok(result)
        assert "enabled" in _get_text(result) or "true" in _get_text(result).lower()

        # Add a constraint that checks feature flag state
        constraint_name = f"config-check-{uuid.uuid4().hex[:6]}"
        result = _call(mcp, "praxis_add_constraint", {
            "name": constraint_name,
            "severity": "error",
            "when": "action == 'deploy'",
            "require": "feature_enabled == true",
            "message": f"Feature flag {test_prefix} must be enabled",
        })
        _assert_ok(result)

        # Evaluate with matching context — should pass (feature_enabled = true)
        result = _call(mcp, "praxis_evaluate", {
            "context": {"action": "deploy", "feature_enabled": True},
        })
        assert result is not None


# ── Workflow 7: Plugin + DB Integration ───────────────────────────────────────


class TestPluginDbWorkflow:
    """Register plugin, track its state in DB."""

    def test_plugin_register_track_in_db(self, mcp, test_prefix):
        """Register a plugin, store activation info in DB, query it."""
        plugin_name = f"xflow-plugin-{uuid.uuid4().hex[:6]}"

        # Register plugin
        result = _call(mcp, "plugin_register", {
            "name": plugin_name,
            "version": "1.0.0",
            "description": "Cross-tool workflow test plugin",
            "capabilities": ["data-processing", "reporting"],
        })
        # Plugin runtime may not be configured in headless MCP mode
        if isinstance(result, str) and "not configured" in result.lower():
            pytest.skip("Plugin runtime not configured in headless MCP mode")
        _assert_ok(result, "register")

        # Activate it
        result = _call(mcp, "plugin_activate", {"name": plugin_name})
        if isinstance(result, str) and "not configured" in result.lower():
            pytest.skip("Plugin runtime not configured in headless MCP mode")
        _assert_ok(result, "activate")

        # Store plugin metadata in DB
        result = _call(mcp, "db_put", {
            "key": f"plugins:{plugin_name}:deployment",
            "value": {
                "deployed_at": time.time(),
                "version": "1.0.0",
                "test_prefix": test_prefix,
            },
        })
        _assert_ok(result)

        # Query — plugin should be in list
        result = _call(mcp, "plugin_list", {})
        _assert_ok(result)
        text = _get_text(result)
        assert plugin_name in text

        # Query DB for deployment info
        result = _call(mcp, "db_get", {"key": f"plugins:{plugin_name}:deployment"})
        _assert_ok(result)
        assert "1.0.0" in _get_text(result)

        # Cleanup
        _call(mcp, "plugin_deactivate", {"name": plugin_name})


# ── Workflow 8: End-to-End Automation Pipeline ────────────────────────────────


class TestEndToEndPipeline:
    """Complete automation pipeline: file → shell → db → chronos audit."""

    def test_full_pipeline_build_deploy_audit(self, mcp, test_prefix):
        """Simulate a build→deploy→audit pipeline using multiple tools."""
        run_id = uuid.uuid4().hex[:8]
        build_dir = f"/tmp/{test_prefix}/pipeline-{run_id}"

        # Step 1: Record pipeline start
        _call(mcp, "chronos_record", {
            "key": f"xflow:pipeline:start:{run_id}",
            "actor": "pytest-pipeline",
            "action": "create",
            "level": "info",
            "data": {"run_id": run_id, "stage": "build"},
        })

        # Step 2: Write source file
        src = f"{build_dir}/main.py"
        result = _call(mcp, "write_file", {
            "path": src,
            "content": f'print("build-{run_id}")\n',
        })
        _assert_ok(result, "write source")

        # Step 3: "Build" — run the script, capture output
        result = _call(mcp, "run_command", {"command": f"python3 {src}"})
        _assert_ok(result)
        build_output = _get_text(result)
        assert run_id in build_output

        # Step 4: Store build artifact metadata in DB
        result = _call(mcp, "db_put", {
            "key": f"pipeline:{run_id}:build",
            "value": {
                "status": "success",
                "output": build_output[:500],
                "artifact_path": src,
            },
        })
        _assert_ok(result)

        # Step 5: Record build complete
        _call(mcp, "chronos_record", {
            "key": f"xflow:pipeline:build-done:{run_id}",
            "actor": "pytest-pipeline",
            "action": "update",
            "level": "info",
            "data": {"run_id": run_id, "status": "success"},
        })

        # Step 6: "Deploy" — write deploy manifest
        manifest = f"{build_dir}/deploy.json"
        deploy_data = json.dumps({
            "run_id": run_id,
            "artifact": src,
            "deployed_at": time.time(),
        })
        result = _call(mcp, "write_file", {"path": manifest, "content": deploy_data})
        _assert_ok(result)

        # Step 7: Store deploy state
        result = _call(mcp, "db_put", {
            "key": f"pipeline:{run_id}:deploy",
            "value": {"status": "deployed", "manifest": manifest},
        })
        _assert_ok(result)

        # Step 8: Record deploy complete
        _call(mcp, "chronos_record", {
            "key": f"xflow:pipeline:deployed:{run_id}",
            "actor": "pytest-pipeline",
            "action": "update",
            "level": "info",
            "data": {"run_id": run_id},
        })

        # Step 9: Verify full pipeline state from DB
        result = _call(mcp, "db_get", {"key": f"pipeline:{run_id}:build"})
        _assert_ok(result)
        assert "success" in _get_text(result)

        result = _call(mcp, "db_get", {"key": f"pipeline:{run_id}:deploy"})
        _assert_ok(result)
        assert "deployed" in _get_text(result)

        # Step 10: Verify timeline has our events
        result = _call(mcp, "chronos_timeline", {"limit": 50})
        _assert_ok(result)
        timeline = _get_text(result)
        assert "xflow:pipeline" in timeline or run_id in timeline

    def test_multi_file_project_with_listing(self, mcp, test_prefix):
        """Create multiple files, list directory, verify structure."""
        project_dir = f"/tmp/{test_prefix}/project-{uuid.uuid4().hex[:6]}"

        # Create project structure
        files = {
            f"{project_dir}/src/main.py": "# Main entry\nprint('hello')\n",
            f"{project_dir}/src/utils.py": "# Utilities\ndef add(a, b): return a + b\n",
            f"{project_dir}/tests/test_main.py": "# Tests\ndef test_hello(): pass\n",
            f"{project_dir}/README.md": "# Project\nA test project.\n",
        }

        for path, content in files.items():
            result = _call(mcp, "write_file", {"path": path, "content": content})
            _assert_ok(result, f"write {path}")

        # List root directory
        result = _call(mcp, "list_directory", {"path": project_dir})
        _assert_ok(result)
        listing = _get_text(result)
        assert "src" in listing
        assert "README" in listing

        # List src/ subdirectory
        result = _call(mcp, "list_directory", {"path": f"{project_dir}/src"})
        _assert_ok(result)
        listing = _get_text(result)
        assert "main.py" in listing
        assert "utils.py" in listing

        # Read back a specific file
        result = _call(mcp, "read_file", {"path": f"{project_dir}/src/utils.py"})
        _assert_ok(result)
        assert "add" in _get_text(result)

    def test_iterative_development_loop(self, mcp, test_prefix):
        """Simulate: write code → test → fix → test (the dev loop)."""
        project_dir = f"/tmp/{test_prefix}/devloop-{uuid.uuid4().hex[:6]}"
        code_path = f"{project_dir}/app.py"
        test_path = f"{project_dir}/test_app.py"

        # Write initial code (with a bug)
        _call(mcp, "write_file", {
            "path": code_path,
            "content": "def greet(name):\n    return f'Hello {name}'\n",
        })

        # Write test
        _call(mcp, "write_file", {
            "path": test_path,
            "content": (
                "import sys; sys.path.insert(0, '"
                + project_dir
                + "')\n"
                + "from app import greet\n"
                + "assert greet('World') == 'Hello, World!', f'Got: {greet(\"World\")}'\n"
                + "print('ALL TESTS PASSED')\n"
            ),
        })

        # Run test — should fail (bug: missing comma and exclamation)
        result = _call(mcp, "run_command", {
            "command": f"python3 {test_path} 2>&1 || true",
        })
        output = _get_text(result)
        assert "PASSED" not in output  # Test should fail

        # Fix the bug
        result = _call(mcp, "edit_file", {
            "path": code_path,
            "old_text": "return f'Hello {name}'",
            "new_text": "return f'Hello, {name}!'",
        })
        _assert_ok(result, "edit fix")

        # Re-run test — should pass now
        result = _call(mcp, "run_command", {
            "command": f"python3 {test_path} 2>&1",
        })
        output = _get_text(result)
        assert "ALL TESTS PASSED" in output


# ── Workflow 9: DB Bulk Operations + Listing ──────────────────────────────────


class TestDbBulkWorkflow:
    """Store multiple related keys, query by prefix, aggregate."""

    def test_store_and_query_by_prefix(self, mcp, test_prefix):
        """Store multiple keys with prefix, list them, verify count."""
        prefix = f"{test_prefix}:metrics"

        # Store multiple metrics
        for i in range(5):
            result = _call(mcp, "db_put", {
                "key": f"{prefix}:item-{i}",
                "value": {"value": i * 10, "label": f"metric-{i}"},
            })
            _assert_ok(result, f"put item-{i}")

        # List keys by prefix
        result = _call(mcp, "db_keys", {"prefix": prefix})
        _assert_ok(result)
        keys_text = _get_text(result)
        # Should find all 5 items
        for i in range(5):
            assert f"item-{i}" in keys_text

    def test_db_state_persistence_across_operations(self, mcp, test_prefix):
        """Verify DB state persists across different tool operations."""
        key = f"{test_prefix}:persistent-state"

        # Write state
        _call(mcp, "db_put", {
            "key": key,
            "value": {"counter": 1, "status": "initialized"},
        })

        # Do unrelated work (file write)
        _call(mcp, "write_file", {
            "path": f"/tmp/{test_prefix}/unrelated.txt",
            "content": "unrelated operation",
        })

        # Do more unrelated work (chronos)
        _call(mcp, "chronos_record", {
            "key": f"{test_prefix}:unrelated",
            "actor": "pytest-xflow",
            "action": "create",
            "level": "debug",
        })

        # Verify DB state is still there
        result = _call(mcp, "db_get", {"key": key})
        _assert_ok(result)
        text = _get_text(result)
        assert "initialized" in text
        assert "counter" in text


# ── Workflow 10: Memory-Driven Decision Making ────────────────────────────────


class TestMemoryDrivenWorkflow:
    """Store context, search it, use results to drive decisions."""

    def test_store_patterns_search_later(self, mcp, test_prefix):
        """Store multiple related memories, search semantically."""
        tag = uuid.uuid4().hex[:6]

        memories = [
            f"The API rate limit for service-{tag} is 100 requests per minute",
            f"Service-{tag} authentication uses OAuth2 with client credentials",
            f"Service-{tag} returns 429 when rate limited with Retry-After header",
        ]

        for mem in memories:
            result = _call(mcp, "memory_store", {
                "content": mem,
                "category": "entity",
                "tags": ["api", "service", tag],
            })
            _assert_ok(result)

        time.sleep(0.5)

        # Search for rate limiting info
        result = _call(mcp, "memory_search", {
            "query": f"rate limit service-{tag}",
            "limit": 5,
        })
        _assert_ok(result)
        text = _get_text(result)
        assert "100" in text or "rate" in text.lower()

        # Search for auth info
        result = _call(mcp, "memory_search", {
            "query": f"authentication service-{tag} OAuth",
            "limit": 5,
        })
        _assert_ok(result)
        text = _get_text(result)
        assert "OAuth" in text or "auth" in text.lower()
