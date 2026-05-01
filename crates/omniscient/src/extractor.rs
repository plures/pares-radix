//! Content extractors — one per ContentClass.
//!
//! Each extractor takes raw file bytes and produces:
//! - Extracted text (for embedding)
//! - Metadata (language, page count, etc.)
//! - Security profile (for binaries)

use crate::file_node::{ContentClass, SecurityProfile};

/// Result of content extraction (Pass 1).
#[derive(Debug, Clone)]
pub struct Extraction {
    /// Extracted text content (for embedding)
    pub text: Option<String>,
    /// Additional metadata
    pub metadata: Vec<(String, String)>,
    /// Security profile (populated for binaries)
    pub security: Option<SecurityProfile>,
}

/// Maximum text to extract before truncation (128KB).
const MAX_TEXT_LEN: usize = 128 * 1024;

/// Extract content based on content class.
pub fn extract(class: &ContentClass, data: &[u8], path: &str) -> Extraction {
    match class {
        ContentClass::Text => extract_text(data),
        ContentClass::Code { language } => extract_code(data, language),
        ContentClass::Pdf => extract_pdf(data, path),
        ContentClass::Image => extract_image(path),
        ContentClass::Office => extract_office(data, path),
        ContentClass::Binary => extract_binary(data, path),
        ContentClass::Audio => extract_audio(path),
        ContentClass::Video => extract_video(path),
        ContentClass::Archive => extract_archive(path),
        ContentClass::Unknown => Extraction {
            text: None,
            metadata: vec![],
            security: None,
        },
    }
}

fn extract_text(data: &[u8]) -> Extraction {
    let text = String::from_utf8_lossy(data);
    let truncated = if text.len() > MAX_TEXT_LEN {
        text[..MAX_TEXT_LEN].to_string()
    } else {
        text.into_owned()
    };
    Extraction {
        text: Some(truncated),
        metadata: vec![
            ("lines".into(), data.iter().filter(|&&b| b == b'\n').count().to_string()),
        ],
        security: None,
    }
}

fn extract_code(data: &[u8], language: &str) -> Extraction {
    let owned = String::from_utf8_lossy(data).into_owned();
    
    // Compute line-derived stats before consuming owned
    let total_lines;
    let code_lines;
    let signatures;
    {
        let lines: Vec<&str> = owned.lines().collect();
        total_lines = lines.len();
        code_lines = lines.iter().filter(|l| {
            let trimmed = l.trim();
            !trimmed.is_empty() && !trimmed.starts_with("//") && !trimmed.starts_with('#')
        }).count();
        signatures = lines.iter().filter_map(|line| {
            let trimmed = line.trim();
            if trimmed.starts_with("pub fn ")
                || trimmed.starts_with("fn ")
                || trimmed.starts_with("pub struct ")
                || trimmed.starts_with("struct ")
                || trimmed.starts_with("pub enum ")
                || trimmed.starts_with("pub trait ")
                || trimmed.starts_with("class ")
                || trimmed.starts_with("def ")
                || trimmed.starts_with("export function ")
                || trimmed.starts_with("export const ")
                || trimmed.starts_with("function ")
            {
                Some(trimmed.to_string())
            } else {
                None
            }
        }).collect::<Vec<_>>();
    } // lines dropped here

    let sig_count = signatures.len();

    let truncated = if owned.len() > MAX_TEXT_LEN {
        owned[..MAX_TEXT_LEN].to_string()
    } else {
        owned
    };

    Extraction {
        text: Some(truncated),
        metadata: vec![
            ("language".into(), language.to_string()),
            ("total_lines".into(), total_lines.to_string()),
            ("code_lines".into(), code_lines.to_string()),
            ("signatures".into(), sig_count.to_string()),
        ],
        security: None,
    }
}

fn extract_pdf(_data: &[u8], path: &str) -> Extraction {
    // Attempt pdftotext extraction via command
    let output = std::process::Command::new("pdftotext")
        .args([path, "-"])
        .output();

    let text = match output {
        Ok(out) if out.status.success() => {
            let t = String::from_utf8_lossy(&out.stdout).into_owned();
            if t.len() > MAX_TEXT_LEN { t[..MAX_TEXT_LEN].to_string() } else { t }
        }
        _ => return Extraction {
            text: None,
            metadata: vec![("error".into(), "pdftotext not available".into())],
            security: None,
        },
    };

    Extraction {
        text: Some(text),
        metadata: vec![],
        security: None,
    }
}

fn extract_image(_path: &str) -> Extraction {
    // Pass 1: no text extraction from images (needs CLIP in Pass 2)
    // Just record dimensions if possible
    Extraction {
        text: None,
        metadata: vec![("requires_clip".into(), "true".into())],
        security: None,
    }
    // Pass 2 will: CLIP embed → caption → re-embed caption
    // Keeping this as a stub for now
    // TODO: integrate CLIP ViT-B/32 or Florence-2 for image understanding
}

fn extract_office(_data: &[u8], path: &str) -> Extraction {
    // Try python-based extraction
    let script = format!(
        "import sys\ntry:\n from docx import Document\n doc = Document('{}')\n print('\\n'.join(p.text for p in doc.paragraphs))\nexcept:\n try:\n  import openpyxl\n  wb = openpyxl.load_workbook('{}', read_only=True)\n  [print('\\t'.join(str(c) for c in row if c)) for ws in wb.worksheets for row in ws.iter_rows(values_only=True)]\n except:\n  print('')",
        path, path
    );
    let output = std::process::Command::new("python3")
        .args(["-c", &script])
        .output();

    let text = match output {
        Ok(out) if out.status.success() => {
            let t = String::from_utf8_lossy(&out.stdout).into_owned();
            if t.trim().is_empty() { None } else { Some(t) }
        }
        _ => None,
    };

    Extraction {
        text,
        metadata: vec![],
        security: None,
    }
}

fn extract_binary(data: &[u8], _path: &str) -> Extraction {
    let mut security = SecurityProfile::default();

    // Shannon entropy
    let mut freq = [0u64; 256];
    for &byte in data {
        freq[byte as usize] += 1;
    }
    let len = data.len() as f64;
    let entropy: f64 = freq.iter()
        .filter(|&&f| f > 0)
        .map(|&f| {
            let p = f as f64 / len;
            -p * p.log2()
        })
        .sum();
    security.entropy = entropy as f32;

    // High entropy = packed/encrypted (normal code ~5-6, packed ~7.5+)
    if entropy > 7.5 {
        security.anomalies.push("high-entropy: possibly packed or encrypted".into());
        security.risk_score += 0.3;
    }

    // Extract printable strings (like `strings` command)
    let mut strings = Vec::new();
    let mut current = String::new();
    for &byte in data {
        if byte.is_ascii_graphic() || byte == b' ' {
            current.push(byte as char);
        } else {
            if current.len() >= 6 {
                strings.push(std::mem::take(&mut current));
            }
            current.clear();
        }
    }
    if current.len() >= 6 {
        strings.push(current);
    }

    // Detect imports/capabilities from strings
    let network_indicators = ["socket", "connect", "bind", "listen", "send", "recv",
        "http", "https", "curl", "wget", "dns", "getaddrinfo"];
    let file_indicators = ["fopen", "fwrite", "readdir", "unlink", "chmod", "CreateFile"];
    let crypto_indicators = ["aes", "rsa", "sha256", "encrypt", "decrypt", "private_key"];
    let exec_indicators = ["exec", "system", "popen", "CreateProcess", "ShellExecute"];

    for s in &strings {
        let lower = s.to_lowercase();
        for ind in &network_indicators {
            if lower.contains(ind) {
                if !security.capabilities.contains(&"network".to_string()) {
                    security.capabilities.push("network".into());
                }
                break;
            }
        }
        for ind in &file_indicators {
            if lower.contains(ind) {
                if !security.capabilities.contains(&"filesystem".to_string()) {
                    security.capabilities.push("filesystem".into());
                }
                break;
            }
        }
        for ind in &crypto_indicators {
            if lower.contains(ind) {
                if !security.capabilities.contains(&"crypto".to_string()) {
                    security.capabilities.push("crypto".into());
                }
                break;
            }
        }
        for ind in &exec_indicators {
            if lower.contains(ind) {
                if !security.capabilities.contains(&"exec".to_string()) {
                    security.capabilities.push("exec".into());
                    security.risk_score += 0.2;
                }
                break;
            }
        }
    }

    // Clamp risk score
    security.risk_score = security.risk_score.min(1.0);

    // Use extracted strings as "text" for embedding
    let text = if strings.is_empty() {
        None
    } else {
        let joined = strings.join(" ");
        Some(if joined.len() > MAX_TEXT_LEN {
            joined[..MAX_TEXT_LEN].to_string()
        } else {
            joined
        })
    };

    Extraction {
        text,
        metadata: vec![
            ("string_count".into(), strings.len().to_string()),
            ("entropy".into(), format!("{:.2}", entropy)),
        ],
        security: Some(security),
    }
}

fn extract_audio(_path: &str) -> Extraction {
    // Pass 1: metadata only. Pass 2 will run whisper.
    Extraction {
        text: None,
        metadata: vec![("requires_whisper".into(), "true".into())],
        security: None,
    }
}

fn extract_video(_path: &str) -> Extraction {
    // Pass 1: metadata only. Pass 2 will extract keyframes + whisper audio.
    Extraction {
        text: None,
        metadata: vec![("requires_keyframe_extraction".into(), "true".into())],
        security: None,
    }
}

fn extract_archive(path: &str) -> Extraction {
    // List archive contents as text
    let output = std::process::Command::new("tar")
        .args(["tf", path])
        .output()
        .or_else(|_| {
            std::process::Command::new("unzip")
                .args(["-l", path])
                .output()
        });

    let text = match output {
        Ok(out) if out.status.success() => {
            Some(String::from_utf8_lossy(&out.stdout).into_owned())
        }
        _ => None,
    };

    Extraction {
        text,
        metadata: vec![],
        security: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_text() {
        let data = b"Hello, world!\nThis is a test file.\n";
        let result = extract_text(data);
        assert!(result.text.unwrap().contains("Hello"));
        assert_eq!(result.metadata[0], ("lines".into(), "2".into()));
    }

    #[test]
    fn test_extract_code_rust() {
        let data = b"pub fn main() {\n    println!(\"hello\");\n}\n\npub struct Config {\n    pub name: String,\n}\n";
        let result = extract_code(data, "rust");
        let text = result.text.unwrap();
        assert!(text.contains("pub fn main"));
        // Should detect 2 signatures (pub fn main, pub struct Config)
        let sigs: usize = result.metadata.iter()
            .find(|(k, _)| k == "signatures")
            .map(|(_, v)| v.parse().unwrap_or(0))
            .unwrap_or(0);
        assert_eq!(sigs, 2);
    }

    #[test]
    fn test_extract_binary_entropy() {
        // Low entropy (all zeros)
        let data = vec![0u8; 1000];
        let result = extract_binary(&data, "test");
        assert!(result.security.as_ref().unwrap().entropy < 1.0);

        // High entropy (random-ish)
        let data: Vec<u8> = (0..1000).map(|i| (i * 37 + 13) as u8).collect();
        let result = extract_binary(&data, "test");
        assert!(result.security.as_ref().unwrap().entropy > 5.0);
    }

    #[test]
    fn test_extract_binary_capabilities() {
        let data = b"some code here socket connect bind listen network stuff and CreateProcess for exec";
        let result = extract_binary(data, "test");
        let sec = result.security.unwrap();
        assert!(sec.capabilities.contains(&"network".to_string()));
        assert!(sec.capabilities.contains(&"exec".to_string()));
        assert!(sec.risk_score > 0.0);
    }

    #[test]
    fn test_extract_binary_strings() {
        let mut data = vec![0u8; 100];
        data.extend_from_slice(b"HelloWorldFunction");
        data.extend_from_slice(&[0u8; 50]);
        data.extend_from_slice(b"AnotherString");
        let result = extract_binary(&data, "test");
        let text = result.text.unwrap();
        assert!(text.contains("HelloWorldFunction"));
        assert!(text.contains("AnotherString"));
    }
}
