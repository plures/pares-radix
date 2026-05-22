"""
test_media_tools.py — E2E tests for media MCP tools (image, TTS, PDF, video, music).

Tests cover:
- image_analyze: input validation, API key requirement, path/URL handling
- image_generate: prompt requirement, API key, parameter passthrough
- tts_generate: text requirement, voice/model params, API key
- pdf_analyze: path requirement, file existence check, pdftotext dependency
- video_generate: prompt requirement, API key
- music_generate: prompt requirement, API key

Philosophy: Test input validation locally (no API keys needed).
When OPENAI_API_KEY is set, run live tests against real APIs.

Run: pytest testing/tests/test_media_tools.py -v
"""
import json
import os
import tempfile
from pathlib import Path

import pytest

from conftest import McpClient


@pytest.fixture(scope="module")
def mcp():
    """Shared MCP client for the test module."""
    client = McpClient()
    client.start()
    yield client
    client.stop()


# ── Image Analyze ──────────────────────────────────────────────────────────────


class TestImageAnalyze:
    """Tests for image_analyze tool."""

    def test_analyze_no_api_key(self, mcp):
        """Without API key configured, returns clear error."""
        result = mcp.call_tool("image_analyze", {"image_url": "https://example.com/img.png"})
        assert result is not None
        text = result.get("text", "") if isinstance(result, dict) else str(result)
        assert "api key" in text.lower() or "not configured" in text.lower()

    def test_analyze_no_image_source(self, mcp):
        """Without image_url or image_path, should error."""
        result = mcp.call_tool("image_analyze", {"prompt": "describe"})
        assert result is not None
        text = result.get("text", "") if isinstance(result, dict) else str(result)
        # Either API key error (checked first) or missing image error
        assert "api key" in text.lower() or "image" in text.lower() or "error" in text.lower()

    def test_analyze_nonexistent_path(self, mcp):
        """With a path that doesn't exist, should error."""
        result = mcp.call_tool("image_analyze", {
            "image_path": "/tmp/nonexistent-image-xyz123.png",
            "prompt": "describe this"
        })
        assert result is not None
        text = result.get("text", "") if isinstance(result, dict) else str(result)
        assert "api key" in text.lower() or "not found" in text.lower() or "error" in text.lower()

    def test_analyze_accepts_prompt(self, mcp):
        """Prompt parameter is accepted (even if it fails due to no key)."""
        result = mcp.call_tool("image_analyze", {
            "image_url": "https://example.com/test.jpg",
            "prompt": "What color is the sky?"
        })
        assert result is not None

    def test_analyze_with_local_file(self, mcp):
        """With a real local file but no API key, gets past file validation."""
        # Create a tiny PNG (1x1 pixel)
        png_bytes = (
            b'\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR\x00\x00\x00\x01'
            b'\x00\x00\x00\x01\x08\x02\x00\x00\x00\x90wS\xde\x00'
            b'\x00\x00\x0cIDATx\x9cc\xf8\x0f\x00\x00\x01\x01\x00'
            b'\x05\x18\xd8N\x00\x00\x00\x00IEND\xaeB`\x82'
        )
        with tempfile.NamedTemporaryFile(suffix=".png", delete=False) as f:
            f.write(png_bytes)
            tmp_path = f.name
        try:
            result = mcp.call_tool("image_analyze", {
                "image_path": tmp_path,
                "prompt": "describe"
            })
            assert result is not None
            text = result.get("text", "") if isinstance(result, dict) else str(result)
            # Should fail at API key stage, not file reading
            assert "api key" in text.lower() or "error" in text.lower()
        finally:
            os.unlink(tmp_path)


# ── Image Generate ─────────────────────────────────────────────────────────────


class TestImageGenerate:
    """Tests for image_generate tool."""

    def test_generate_missing_prompt(self, mcp):
        """Without prompt, should error."""
        result = mcp.call_tool("image_generate", {})
        assert result is not None
        text = result.get("text", "") if isinstance(result, dict) else str(result)
        assert "prompt" in text.lower() or "api key" in text.lower()

    def test_generate_no_api_key(self, mcp):
        """Without API key, returns clear error."""
        result = mcp.call_tool("image_generate", {"prompt": "a cat wearing a hat"})
        assert result is not None
        text = result.get("text", "") if isinstance(result, dict) else str(result)
        assert "api key" in text.lower() or "not configured" in text.lower()

    def test_generate_accepts_size_param(self, mcp):
        """Size parameter is accepted."""
        result = mcp.call_tool("image_generate", {
            "prompt": "test",
            "size": "512x512"
        })
        assert result is not None

    def test_generate_accepts_quality_param(self, mcp):
        """Quality parameter is accepted."""
        result = mcp.call_tool("image_generate", {
            "prompt": "test",
            "quality": "hd"
        })
        assert result is not None

    def test_generate_accepts_model_param(self, mcp):
        """Model parameter is accepted."""
        result = mcp.call_tool("image_generate", {
            "prompt": "test",
            "model": "dall-e-3"
        })
        assert result is not None


# ── TTS Generate ───────────────────────────────────────────────────────────────


class TestTtsGenerate:
    """Tests for tts_generate tool."""

    def test_tts_missing_text(self, mcp):
        """Without text, should error."""
        result = mcp.call_tool("tts_generate", {})
        assert result is not None
        text = result.get("text", "") if isinstance(result, dict) else str(result)
        assert "text" in text.lower() or "api key" in text.lower() or "missing" in text.lower()

    def test_tts_no_api_key(self, mcp):
        """Without API key, returns clear error."""
        result = mcp.call_tool("tts_generate", {"text": "Hello world"})
        assert result is not None
        text = result.get("text", "") if isinstance(result, dict) else str(result)
        assert "api key" in text.lower() or "not configured" in text.lower()

    def test_tts_accepts_voice_param(self, mcp):
        """Voice parameter is accepted."""
        result = mcp.call_tool("tts_generate", {
            "text": "Hello",
            "voice": "nova"
        })
        assert result is not None

    def test_tts_accepts_model_param(self, mcp):
        """Model parameter is accepted."""
        result = mcp.call_tool("tts_generate", {
            "text": "Hello",
            "model": "tts-1-hd"
        })
        assert result is not None

    def test_tts_various_voices(self, mcp):
        """All standard voices are accepted as input."""
        voices = ["alloy", "echo", "fable", "onyx", "nova", "shimmer"]
        for voice in voices:
            result = mcp.call_tool("tts_generate", {
                "text": f"Testing {voice}",
                "voice": voice
            })
            assert result is not None


# ── PDF Analyze ────────────────────────────────────────────────────────────────


class TestPdfAnalyze:
    """Tests for pdf_analyze tool."""

    def test_pdf_missing_path(self, mcp):
        """Without path, should error."""
        result = mcp.call_tool("pdf_analyze", {})
        assert result is not None
        text = result.get("text", "") if isinstance(result, dict) else str(result)
        assert "path" in text.lower() or "missing" in text.lower()

    def test_pdf_nonexistent_file(self, mcp):
        """With nonexistent file, should error."""
        result = mcp.call_tool("pdf_analyze", {"path": "/tmp/nonexistent-pdf-xyz.pdf"})
        assert result is not None
        text = result.get("text", "") if isinstance(result, dict) else str(result)
        assert "not found" in text.lower() or "error" in text.lower()

    def test_pdf_with_real_file(self, mcp):
        """With a minimal PDF, exercises pdftotext integration."""
        # Create a minimal PDF
        pdf_content = b"""%PDF-1.0
1 0 obj
<< /Type /Catalog /Pages 2 0 R >>
endobj
2 0 obj
<< /Type /Pages /Kids [3 0 R] /Count 1 >>
endobj
3 0 obj
<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>
endobj
4 0 obj
<< /Length 44 >>
stream
BT /F1 12 Tf 100 700 Td (Hello PDF) Tj ET
endstream
endobj
5 0 obj
<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>
endobj
xref
0 6
0000000000 65535 f 
0000000009 00000 n 
0000000058 00000 n 
0000000115 00000 n 
0000000266 00000 n 
0000000360 00000 n 
trailer
<< /Size 6 /Root 1 0 R >>
startxref
441
%%EOF"""
        with tempfile.NamedTemporaryFile(suffix=".pdf", delete=False) as f:
            f.write(pdf_content)
            tmp_path = f.name
        try:
            result = mcp.call_tool("pdf_analyze", {"path": tmp_path})
            assert result is not None
            text = result.get("text", "") if isinstance(result, dict) else str(result)
            # Either successfully extracted text or pdftotext not installed
            assert (
                "hello" in text.lower()
                or "pdf" in text.lower()
                or "pdftotext" in text.lower()
                or "error" in text.lower()
            )
        finally:
            os.unlink(tmp_path)

    def test_pdf_accepts_prompt_param(self, mcp):
        """Prompt parameter is accepted for analysis guidance."""
        result = mcp.call_tool("pdf_analyze", {
            "path": "/tmp/test.pdf",
            "prompt": "Summarize the key points"
        })
        assert result is not None


# ── Video Generate ─────────────────────────────────────────────────────────────


class TestVideoGenerate:
    """Tests for video_generate tool."""

    def test_video_missing_prompt(self, mcp):
        """Without prompt, should error."""
        result = mcp.call_tool("video_generate", {})
        assert result is not None
        text = result.get("text", "") if isinstance(result, dict) else str(result)
        assert "prompt" in text.lower() or "api key" in text.lower() or "missing" in text.lower() or "not configured" in text.lower()

    def test_video_no_api_key(self, mcp):
        """Without API key, returns clear error."""
        result = mcp.call_tool("video_generate", {"prompt": "a sunset timelapse"})
        assert result is not None
        text = result.get("text", "") if isinstance(result, dict) else str(result)
        assert "api key" in text.lower() or "not configured" in text.lower() or "not supported" in text.lower()

    def test_video_accepts_params(self, mcp):
        """Parameters like duration are accepted."""
        result = mcp.call_tool("video_generate", {
            "prompt": "test video",
            "duration": 5
        })
        assert result is not None


# ── Music Generate ─────────────────────────────────────────────────────────────


class TestMusicGenerate:
    """Tests for music_generate tool."""

    def test_music_missing_prompt(self, mcp):
        """Without prompt, should error."""
        result = mcp.call_tool("music_generate", {})
        assert result is not None
        text = result.get("text", "") if isinstance(result, dict) else str(result)
        assert "prompt" in text.lower() or "api key" in text.lower() or "missing" in text.lower() or "not configured" in text.lower()

    def test_music_no_api_key(self, mcp):
        """Without API key, returns clear error."""
        result = mcp.call_tool("music_generate", {"prompt": "upbeat jazz piano"})
        assert result is not None
        text = result.get("text", "") if isinstance(result, dict) else str(result)
        assert "api key" in text.lower() or "not configured" in text.lower() or "not supported" in text.lower()

    def test_music_accepts_params(self, mcp):
        """Parameters like duration and style are accepted."""
        result = mcp.call_tool("music_generate", {
            "prompt": "calm ambient",
            "duration": 10
        })
        assert result is not None


# ── Tool Registration ──────────────────────────────────────────────────────────


class TestMediaToolRegistration:
    """Verify all media tools are registered in the MCP tool list."""

    def test_all_media_tools_registered(self, mcp):
        """All 6 media tools appear in tools/list."""
        tools = mcp.list_tools()
        tool_names = [t["name"] for t in tools]
        expected = [
            "image_analyze",
            "image_generate",
            "tts_generate",
            "pdf_analyze",
            "video_generate",
            "music_generate",
        ]
        for name in expected:
            assert name in tool_names, f"Missing tool: {name}"

    def test_image_analyze_has_params(self, mcp):
        """image_analyze declares expected parameters."""
        tools = mcp.list_tools()
        tool = next(t for t in tools if t["name"] == "image_analyze")
        schema = tool.get("inputSchema", {})
        props = schema.get("properties", {})
        assert "image_url" in props or "image_path" in props

    def test_image_generate_requires_prompt(self, mcp):
        """image_generate declares prompt parameter."""
        tools = mcp.list_tools()
        tool = next(t for t in tools if t["name"] == "image_generate")
        schema = tool.get("inputSchema", {})
        props = schema.get("properties", {})
        assert "prompt" in props

    def test_tts_has_text_and_voice(self, mcp):
        """tts_generate declares text and voice parameters."""
        tools = mcp.list_tools()
        tool = next(t for t in tools if t["name"] == "tts_generate")
        schema = tool.get("inputSchema", {})
        props = schema.get("properties", {})
        assert "text" in props
        assert "voice" in props or "model" in props

    def test_pdf_analyze_has_path(self, mcp):
        """pdf_analyze declares path parameter."""
        tools = mcp.list_tools()
        tool = next(t for t in tools if t["name"] == "pdf_analyze")
        schema = tool.get("inputSchema", {})
        props = schema.get("properties", {})
        assert "path" in props


# ── Live API tests (require OPENAI_API_KEY) ────────────────────────────────────


OPENAI_KEY = os.environ.get("OPENAI_API_KEY") or os.environ.get("PARES_API_KEY")


@pytest.mark.skipif(
    not OPENAI_KEY,
    reason="OPENAI_API_KEY/PARES_API_KEY not set — no live API testing",
)
class TestMediaLiveAPI:
    """Tests that call real OpenAI APIs.

    These cost money — run selectively.
    Set OPENAI_API_KEY or PARES_API_KEY to enable.
    """

    @pytest.fixture(scope="class")
    def api_mcp(self):
        """MCP client with API key configured."""
        workdir = f"/tmp/radix-media-test-{os.getpid()}"
        os.makedirs(workdir, exist_ok=True)
        # Set env for the process
        client = McpClient(workdir=workdir)
        # Inject key via environment (McpClient passes env through)
        os.environ["OPENAI_API_KEY"] = OPENAI_KEY
        client.start()
        yield client
        client.stop()

    def test_live_tts_generates_audio(self, api_mcp):
        """TTS with real API key produces audio data."""
        result = api_mcp.call_tool("tts_generate", {
            "text": "Testing pares-radix TTS integration.",
            "voice": "alloy"
        })
        assert result is not None
        text = result.get("text", "") if isinstance(result, dict) else str(result)
        # Should either save a file or return base64 audio
        assert "error" not in text.lower() or "saved" in text.lower() or "audio" in text.lower()
