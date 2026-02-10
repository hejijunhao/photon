# Phase 4: Adaptive Zero-Shot Tagging

> **Duration:** 2 weeks
> **Milestone:** `photon process image.jpg` outputs semantic tags with confidence scores using self-organizing vocabulary
> **Reference:** [taxonomy-vision.md](taxonomy-vision.md) for full architectural rationale

---

## Overview

This phase implements zero-shot image classification using SigLIP's text encoder and a WordNet-derived vocabulary of ~80,000 nouns. Instead of a manually authored taxonomy, Photon scores every image against the full vocabulary via matrix multiplication, uses WordNet's built-in hypernym hierarchy for deduplication, and progressively self-organizes the vocabulary to the user's library over time.

**Key design principles:**
- No LLM dependency — purely algorithmic using SigLIP embeddings, WordNet, and Rust logic
- No training phase, no batch rebuilds — the vocabulary self-organizes continuously
- Brute-force flat scoring against the full active vocabulary (~1-5ms per image)
- Hierarchy is display-time only — WordNet hypernym chains organize output, not scoring

---

## Prerequisites

- Phase 3 completed (SigLIP image embeddings working)
- **`pooler_output` fix applied** — Phase 3's `SigLipSession::embed()` uses `last_hidden_state` (1st output), which works for image-image similarity but breaks cross-modal alignment. Must switch to `pooler_output` (2nd output) before any tagging work. See 4a.pre below.
- SigLIP models downloaded via `photon models download` (interactive model selection — see 4a.0)
- SigLIP text encoder model (`text_model.onnx`, fp32) + tokenizer (`tokenizer.json`) downloaded
- **Note:** fp16 text encoder crashes on aarch64/Asahi Linux — must use fp32 variant (~441MB)

---

## Background: How It Works

1. **Generate image embedding** (done in Phase 3) — 768-dim vector
2. **Score against vocabulary** — dot product of image embedding against all term embeddings (N x 768 matrix) → N similarity scores in ~1-5ms
3. **Filter by min_confidence** — discard low-scoring terms
4. **Deduplicate via WordNet hierarchy** — if "labrador" (0.87) and "dog" (0.81) both pass, suppress "dog" since it's an ancestor
5. **Emit tags** — sorted by confidence, limited to max_tags

No tree traversal for scoring. No clustering. Each image scored independently against the flat vocabulary.

---

## Implementation Sub-Phases

Phase 4 is split into sub-phases. **Phase 4a alone delivers the core value.** Each subsequent phase is an optimization.

| Sub-Phase | What | Complexity | Value |
|-----------|------|------------|-------|
| **4a** | Flat brute-force scoring against shipped vocabulary (encode all at startup, no pruning) | Medium | Working auto-taxonomy with zero config |
| ↳ **4a.pre** | Fix vision encoder to use `pooler_output` instead of `last_hidden_state` | Low | Prerequisite — cross-modal alignment |
| ↳ **4a.0** | Multi-resolution model download (224/384 selection), text encoder + tokenizer, `--quality` flag | Low-Medium | User choice between speed and accuracy |
| **4b** | Progressive encoding (chunked startup, background encoding) | Medium | Fast startup (2-3s instead of 2-3min) |
| **4c** | Relevance pruning (three-pool system, per-term stats) | Medium | Self-organizing vocabulary |
| **4d** | Neighbor expansion (WordNet-driven priority encoding) | Low | Deeper coverage where it matters |
| **4e** | Hierarchy deduplication (ancestor suppression, path display) | Low | Cleaner output |

---

## Phase 4a: Core Flat Scoring

### 4a.pre Fix Vision Encoder Output Selection

**Goal:** Switch `SigLipSession::embed()` from using `last_hidden_state` (1st output) to `pooler_output` (2nd output). This is a prerequisite for cross-modal alignment — without it, image and text embeddings land in different subspaces and zero-shot tagging produces meaningless scores.

**Background:** The Phase 4 spike found that both vision and text ONNX models expose two outputs:

| Output | Shape | Purpose |
|--------|-------|---------|
| `last_hidden_state` | [1, 196, 768] (vision) / [1, seq_len, 768] (text) | Raw transformer output |
| `pooler_output` | [1, 768] | Projected embedding for cross-modal alignment |

The Phase 3 implementation takes the first output and mean-pools it. This works for image-to-image similarity (scores of 0.45–0.59) but produces near-zero cross-modal similarity with no meaningful ranking.

**Steps:**

1. In `crates/photon-core/src/embedding/siglip.rs`, modify `SigLipSession::embed()` to extract `pooler_output` by name (the 2nd output) instead of taking `outputs[0]` and mean-pooling:
   ```rust
   // Before (Phase 3):
   let output_tensor = &outputs[0];  // last_hidden_state [1, 196, 768]
   // ... mean-pool across token dimension ...

   // After:
   let output_tensor = &outputs["pooler_output"];  // [1, 768] — already pooled
   ```
   The `pooler_output` is already a single 768-dim vector per image, so the mean-pooling step is removed entirely.

2. Update L2 normalization to operate on the flat 768-dim vector (simpler than the previous token-dimension reduction).

3. Update existing Phase 3 embedding tests — embedding values will change since we're using a different output. Tests should verify:
   - Output is still 768-dim and L2-normalized
   - Different images produce different embeddings
   - Embeddings are deterministic (same image → same vector)

**Acceptance Criteria:**
- [ ] `SigLipSession::embed()` extracts `pooler_output` by name
- [ ] Mean-pooling code removed (no longer needed)
- [ ] Output is 768-dim, L2-normalized
- [ ] All existing embedding tests pass (updated for new output values)
- [ ] `siglip.rs` is the only file modified in the embedding module

---

### 4a.0 Model Download & Multi-Resolution Support

**Goal:** Upgrade `photon models download` to support two SigLIP vision encoder variants (224 and 384), add interactive model selection, download the shared text encoder + tokenizer, and wire a `--quality` flag into the processing CLI.

#### Background

SigLIP base comes in two input resolutions. Both produce 768-dim embeddings, so the text encoder, label bank, and scoring pipeline are identical downstream — only the vision encoder ONNX file and preprocessing target size differ.

| Variant | Input | Patches | HuggingFace Repo | Vision ONNX | ~Size |
|---------|-------|---------|------------------|-------------|-------|
| **Base 224** (default) | 224×224 | 196 | `Xenova/siglip-base-patch16-224` | `onnx/vision_model.onnx` | ~350MB |
| **Base 384** | 384×384 | 576 | `Xenova/siglip-base-patch16-384` | `onnx/vision_model.onnx` | ~350MB |

Shared across both variants (from the 224 repo):
- Text encoder: `onnx/text_model.onnx` (fp32, ~441MB)
- Tokenizer: `tokenizer.json` (~2MB)

Performance and quality tradeoffs:

| | 224 | 384 |
|---|-----|-----|
| Vision inference (CPU) | ~60-100ms/image | ~200-400ms/image |
| 50 images | ~4-5s | ~12-18s |
| 10K images | ~15 min | ~50 min |
| Zero-shot ImageNet top-1 | ~73-74% | ~76-78% |
| Fine-grained accuracy | Good | Noticeably better |
| Coarse-grained accuracy | Good | ~Same |

#### Interactive Model Selection

`photon models download` presents an interactive selector:

```
Select SigLIP vision model(s) to download:

  ❯ Base (224)    ~350MB  — fast, good for most use cases
    Base (384)    ~350MB  — higher detail, 3-4× slower
    Both          ~700MB  — switch per-run with --quality

  Text encoder + tokenizer (~443MB, fp32) will also be downloaded.
```

Arrow keys to navigate, Enter to select. After selection, downloads proceed with progress reporting.

#### Disk Layout

```
~/.photon/models/
├── siglip-base-patch16/          # 224 variant (default)
│   └── visual.onnx               # ~350MB
├── siglip-base-patch16-384/      # 384 variant (optional)
│   └── visual.onnx               # ~350MB
├── text_model.onnx          # Shared text encoder, fp32 (~441MB)
└── tokenizer.json                # Shared tokenizer (~2MB)
```

Text encoder and tokenizer live at the `models/` root since they're shared across vision variants.

#### Config Changes

Update `EmbeddingConfig` to track the available variant and allow per-run override:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct EmbeddingConfig {
    /// Default model variant ("siglip-base-patch16" or "siglip-base-patch16-384")
    pub model: String,

    /// Image input size — derived from model variant, not set directly
    /// 224 for base, 384 for 384 variant
    pub image_size: u32,

    /// Inference device
    pub device: String,
}

impl Default for EmbeddingConfig {
    fn default() -> Self {
        Self {
            model: "siglip-base-patch16".to_string(),
            image_size: 224,
            device: "cpu".to_string(),
        }
    }
}
```

Add a helper to resolve image size from model name:

```rust
impl EmbeddingConfig {
    pub fn image_size_for_model(model: &str) -> u32 {
        if model.contains("384") { 384 } else { 224 }
    }
}
```

#### Preprocessing Parameterization

Currently `preprocess.rs` hardcodes `SIGLIP_IMAGE_SIZE: u32 = 224`. Change `preprocess()` to accept the target size:

```rust
pub fn preprocess(image: &DynamicImage, image_size: u32) -> Array4<f32> {
    let resized = image.resize_exact(
        image_size,
        image_size,
        image::imageops::FilterType::Lanczos3,
    );
    // ... rest unchanged, using image_size instead of SIGLIP_IMAGE_SIZE
}
```

The constant `SIGLIP_IMAGE_SIZE` becomes a default, and callers pass the configured size from `EmbeddingConfig::image_size`.

#### CLI: `--quality` Flag

Add a `--quality` option to `photon process`:

```
photon process image.jpg                   # uses default (224)
photon process image.jpg --quality high    # uses 384 variant
photon process image.jpg --quality fast    # explicitly uses 224
```

```rust
#[derive(clap::ValueEnum, Clone, Debug, Default)]
pub enum Quality {
    /// Fast processing with base 224 model (default)
    #[default]
    Fast,
    /// Higher detail with base 384 model (~3-4× slower)
    High,
}
```

If the user requests `--quality high` but only the 224 model is downloaded, warn and fall back:

```
WARN: Base 384 model not found. Falling back to 224.
      Run `photon models download` to install additional models.
```

#### Model Download Implementation

Update `models.rs`:

```rust
/// Available SigLIP vision model variants.
const VISION_VARIANTS: &[ModelVariant] = &[
    ModelVariant {
        name: "siglip-base-patch16",
        label: "Base (224)",
        description: "fast, good for most use cases",
        repo: "Xenova/siglip-base-patch16-224",
        remote_path: "onnx/vision_model.onnx",
        size_mb: 350,
    },
    ModelVariant {
        name: "siglip-base-patch16-384",
        label: "Base (384)",
        description: "higher detail, 3-4× slower",
        repo: "Xenova/siglip-base-patch16-384",
        remote_path: "onnx/vision_model.onnx",
        size_mb: 350,
    },
];

/// Shared models (always downloaded).
const TEXT_ENCODER_REPO: &str = "Xenova/siglip-base-patch16-224";
const TEXT_ENCODER_REMOTE: &str = "onnx/text_model.onnx"; // fp32 — fp16 crashes on aarch64
const TOKENIZER_REMOTE: &str = "tokenizer.json";
```

The download flow:
1. Show interactive selector (Base 224 / Base 384 / Both)
2. Download selected vision model(s) into variant-named subdirectories
3. Always download text encoder + tokenizer to `models/` root
4. Update `models list` to show which variants are installed and which is the default

#### `models list` Output

```
Installed models:
  Directory: ~/.photon/models

  Vision encoders:
    - siglip-base-patch16      ready  (default)
    - siglip-base-patch16-384  ready

  Shared:
    - text_model.onnx     ready
    - tokenizer.json           ready
```

#### Acceptance Criteria

- [ ] `photon models download` presents interactive model selection (224 / 384 / Both)
- [ ] Vision models download to variant-named subdirectories
- [ ] Text encoder + tokenizer always downloaded to `models/` root
- [ ] `photon models list` shows installed variants and default
- [ ] `--quality fast|high` flag on `photon process` selects variant
- [ ] Fallback with warning when requested variant isn't installed
- [ ] `preprocess()` accepts configurable image size
- [ ] `EmbeddingConfig` includes `image_size` derived from model variant
- [ ] Existing Phase 3 embedding tests updated for parameterized preprocessing

---

### 4a.1 SigLIP Text Encoder Integration

**Goal:** Load and run the SigLIP text encoder for generating text embeddings from vocabulary terms.

**Steps:**

1. Add `Model` error variant to `PipelineError` in `crates/photon-core/src/error.rs`:
   ```rust
   /// Model loading or initialization failed (not per-image — for model/tokenizer/label bank errors)
   #[error("Model error: {message}")]
   Model { message: String },
   ```
   This is distinct from the existing `Embedding { path, message }` variant, which is for per-image embedding failures. `Model` covers text encoder load failures, tokenizer errors, label bank corruption, and lock poisoning — errors that aren't tied to a specific image file.

2. Add tokenizer dependency:
   ```toml
   tokenizers = "0.20"
   ```

4. Create `crates/photon-core/src/tagging/mod.rs`:
   ```rust
   pub mod text_encoder;
   pub mod scorer;
   pub mod vocabulary;

   pub use scorer::TagScorer;
   pub use vocabulary::Vocabulary;
   ```

5. Create text encoder wrapper:
   ```rust
   // crates/photon-core/src/tagging/text_encoder.rs

   use std::path::Path;
   use std::sync::Mutex;

   use crate::error::PipelineError;

   pub struct SigLipTextEncoder {
       session: Mutex<ort::Session>,
       tokenizer: tokenizers::Tokenizer,
       embedding_dim: usize,
   }

   impl SigLipTextEncoder {
       pub fn new(model_dir: &Path) -> Result<Self, PipelineError> {
           let text_model_path = model_dir.join("text_model.onnx");
           let tokenizer_path = model_dir.join("tokenizer.json");

           // Load ONNX model (same pattern as vision encoder in Phase 3)
           let session = ort::Session::builder()?
               .with_optimization_level(ort::GraphOptimizationLevel::Level3)?
               .commit_from_file(&text_model_path)?;

           let tokenizer = tokenizers::Tokenizer::from_file(&tokenizer_path)
               .map_err(|e| PipelineError::Model {
                   message: format!("Failed to load tokenizer: {}", e),
               })?;

           Ok(Self {
               session: Mutex::new(session),
               tokenizer,
               embedding_dim: 768,
           })
       }

       /// Encode a batch of text strings to normalized embeddings.
       /// Returns Vec of 768-dim f32 vectors.
       pub fn encode_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, PipelineError> {
           let max_length = 64; // SigLIP default

           // Tokenize all texts
           let encodings = self.tokenizer
               .encode_batch(texts.to_vec(), true)
               .map_err(|e| PipelineError::Model {
                   message: format!("Tokenization failed: {}", e),
               })?;

           let batch_size = texts.len();

           // Build flat input tensor (SigLIP text model takes input_ids only — no attention_mask)
           let mut input_ids = vec![0i64; batch_size * max_length];

           for (i, encoding) in encodings.iter().enumerate() {
               let ids = encoding.get_ids();
               for (j, &id) in ids.iter().take(max_length).enumerate() {
                   input_ids[i * max_length + j] = id as i64;
               }
           }

           // Run inference (same Mutex pattern as EmbeddingEngine)
           let session = self.session.lock().map_err(|e| PipelineError::Model {
               message: format!("Text encoder lock poisoned: {}", e),
           })?;

           let input_ids_value = ort::Value::from_array(
               ([batch_size as i64, max_length as i64], input_ids)
           )?;

           let outputs = session.run(
               ort::inputs![input_ids_value]?
           )?;

           // Extract pooler_output (2nd output) — the cross-modal embedding
           // (1st output is last_hidden_state, which is NOT aligned across modalities)
           let output_tensor = &outputs["pooler_output"];
           let flat: Vec<f32> = output_tensor.try_extract_tensor::<f32>()?
               .iter().copied().collect();

           let embeddings: Vec<Vec<f32>> = flat
               .chunks(self.embedding_dim)
               .map(|chunk| Self::l2_normalize(chunk))
               .collect();

           Ok(embeddings)
       }

       /// Encode a single text string.
       pub fn encode(&self, text: &str) -> Result<Vec<f32>, PipelineError> {
           let batch = self.encode_batch(&[text.to_string()])?;
           Ok(batch.into_iter().next().unwrap())
       }

       fn l2_normalize(v: &[f32]) -> Vec<f32> {
           let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
           if norm > 0.0 {
               v.iter().map(|x| x / norm).collect()
           } else {
               v.to_vec()
           }
       }
   }
   ```

**Acceptance Criteria:**
- [ ] Text encoder loads from ONNX file
- [ ] Tokenizer loads and processes text
- [ ] Output embeddings are 768-dim and L2-normalized
- [ ] Batch encoding works for multiple texts
- [ ] Uses Mutex pattern consistent with Phase 3 EmbeddingEngine

---

### 4a.2 Vocabulary Loading

**Goal:** Ship a WordNet-derived vocabulary and load it at startup.

**Vocabulary Files:**

Photon ships two vocabulary files:

```
~/.photon/vocabulary/
  wordnet_nouns.txt       # ~80K nouns from WordNet (~2MB)
  supplemental.txt        # ~500 scene/mood/style/weather/time terms
```

**Format of wordnet_nouns.txt:**
```
# WordNet nouns vocabulary for Photon
# Format: term<TAB>synset_id<TAB>hypernym_chain (pipe-separated)
labrador_retriever	02099712	retriever|sporting_dog|dog|canine|carnivore|mammal|animal|organism|entity
golden_retriever	02099601	retriever|sporting_dog|dog|canine|carnivore|mammal|animal|organism|entity
poodle	02113624	dog|canine|carnivore|mammal|animal|organism|entity
```

**Format of supplemental.txt:**
```
# Supplemental vocabulary (non-noun visual concepts)
# Format: term<TAB>category
beach	scene
kitchen	scene
cityscape	scene
peaceful	mood
dramatic	mood
vintage	style
foggy	weather
sunset	time
```

**Steps:**

1. Create vocabulary loader:
   ```rust
   // crates/photon-core/src/tagging/vocabulary.rs

   use std::collections::HashMap;
   use std::path::Path;

   use crate::error::PipelineError;

   #[derive(Debug, Clone)]
   pub struct VocabTerm {
       pub name: String,
       pub display_name: String,     // "labrador retriever" (underscores replaced)
       pub synset_id: Option<String>,
       pub hypernyms: Vec<String>,   // ancestor chain, most specific first
       pub category: Option<String>, // for supplemental terms
   }

   pub struct Vocabulary {
       terms: Vec<VocabTerm>,
       by_name: HashMap<String, usize>,
   }

   impl Vocabulary {
       /// Load vocabulary from the vocabulary directory.
       pub fn load(vocab_dir: &Path) -> Result<Self, PipelineError> {
           let mut terms = Vec::new();

           // Load WordNet nouns
           let nouns_path = vocab_dir.join("wordnet_nouns.txt");
           if nouns_path.exists() {
               let content = std::fs::read_to_string(&nouns_path)?;
               for line in content.lines() {
                   if line.starts_with('#') || line.trim().is_empty() {
                       continue;
                   }
                   let parts: Vec<&str> = line.split('\t').collect();
                   if parts.len() >= 3 {
                       let name = parts[0].to_string();
                       let display_name = name.replace('_', " ");
                       let synset_id = Some(parts[1].to_string());
                       let hypernyms: Vec<String> = parts[2]
                           .split('|')
                           .map(|s| s.replace('_', " "))
                           .collect();

                       terms.push(VocabTerm {
                           name: name.clone(),
                           display_name,
                           synset_id,
                           hypernyms,
                           category: None,
                       });
                   }
               }
           }

           // Load supplemental terms
           let supp_path = vocab_dir.join("supplemental.txt");
           if supp_path.exists() {
               let content = std::fs::read_to_string(&supp_path)?;
               for line in content.lines() {
                   if line.starts_with('#') || line.trim().is_empty() {
                       continue;
                   }
                   let parts: Vec<&str> = line.split('\t').collect();
                   if parts.len() >= 2 {
                       terms.push(VocabTerm {
                           name: parts[0].to_string(),
                           display_name: parts[0].to_string(),
                           synset_id: None,
                           hypernyms: vec![],
                           category: Some(parts[1].to_string()),
                       });
                   }
               }
           }

           // Build lookup index
           let by_name: HashMap<String, usize> = terms.iter()
               .enumerate()
               .map(|(i, t)| (t.name.clone(), i))
               .collect();

           tracing::info!("Loaded vocabulary: {} terms ({} WordNet, {} supplemental)",
               terms.len(),
               terms.iter().filter(|t| t.synset_id.is_some()).count(),
               terms.iter().filter(|t| t.category.is_some()).count(),
           );

           Ok(Self { terms, by_name })
       }

       pub fn all_terms(&self) -> &[VocabTerm] {
           &self.terms
       }

       pub fn len(&self) -> usize {
           self.terms.len()
       }

       pub fn get(&self, name: &str) -> Option<&VocabTerm> {
           self.by_name.get(name).map(|&i| &self.terms[i])
       }

       /// Get prompt templates for a term.
       /// Averages multiple prompts for better accuracy.
       pub fn prompts_for(&self, term: &VocabTerm) -> Vec<String> {
           let name = &term.display_name;
           vec![
               name.to_string(),
               format!("a photograph of {}", name),
               format!("a photo of a {}", name),
           ]
       }
   }
   ```

2. Add vocabulary directory config:
   ```toml
   [tagging.vocabulary]
   dir = "~/.photon/vocabulary"
   ```

**Acceptance Criteria:**
- [ ] Loads WordNet nouns with synset IDs and hypernym chains
- [ ] Loads supplemental terms with categories
- [ ] Handles missing/empty files gracefully
- [ ] Name lookup works
- [ ] Prompt templates generate multiple variants per term

---

### 4a.3 Vocabulary Embedding (Startup)

**Goal:** Encode all vocabulary terms through SigLIP text encoder at startup and cache to disk.

In Phase 4a, we encode the entire vocabulary upfront (2-3 minutes). Phase 4b will add progressive encoding.

**Steps:**

1. Create label bank (cached term embeddings):
   ```rust
   // crates/photon-core/src/tagging/label_bank.rs

   use std::path::Path;

   use crate::error::PipelineError;

   use super::text_encoder::SigLipTextEncoder;
   use super::vocabulary::{Vocabulary, VocabTerm};

   /// Pre-computed term embeddings for scoring.
   /// Stores a single flat matrix (N x 768, row-major) for efficient dot product.
   /// Individual term embeddings are accessed by indexing into the matrix.
   pub struct LabelBank {
       /// Flat matrix for batch dot product: (N x 768) stored row-major.
       /// ~240MB for 80K terms. No duplicate storage.
       matrix: Vec<f32>,
       embedding_dim: usize,
       term_count: usize,
   }

   impl LabelBank {
       /// Encode all vocabulary terms and build the label bank.
       /// This takes 2-3 minutes for 80K terms on CPU.
       pub fn encode_all(
           vocabulary: &Vocabulary,
           text_encoder: &SigLipTextEncoder,
           batch_size: usize,
       ) -> Result<Self, PipelineError> {
           let terms = vocabulary.all_terms();
           let embedding_dim = 768;
           let mut matrix: Vec<f32> = Vec::with_capacity(terms.len() * embedding_dim);

           tracing::info!("Encoding {} vocabulary terms (this may take a few minutes on first run)...", terms.len());

           // Process in batches
           for (batch_idx, chunk) in terms.chunks(batch_size).enumerate() {
               // For each term, generate prompt variants and average their embeddings
               for term in chunk {
                   let prompts = vocabulary.prompts_for(term);
                   let prompt_embeddings = text_encoder.encode_batch(
                       &prompts.iter().map(|s| s.to_string()).collect::<Vec<_>>()
                   )?;

                   // Average the prompt embeddings
                   let mut averaged = vec![0.0f32; embedding_dim];
                   for emb in &prompt_embeddings {
                       for (i, &v) in emb.iter().enumerate() {
                           averaged[i] += v;
                       }
                   }
                   let n = prompt_embeddings.len() as f32;
                   for v in averaged.iter_mut() {
                       *v /= n;
                   }

                   // Re-normalize after averaging
                   let norm: f32 = averaged.iter().map(|x| x * x).sum::<f32>().sqrt();
                   if norm > 0.0 {
                       for v in averaged.iter_mut() {
                           *v /= norm;
                       }
                   }

                   matrix.extend_from_slice(&averaged);
               }

               if (batch_idx + 1) % 10 == 0 {
                   let encoded = (batch_idx + 1) * batch_size;
                   tracing::info!("  Encoded {}/{} terms", encoded.min(terms.len()), terms.len());
               }
           }

           let term_count = matrix.len() / embedding_dim;
           tracing::info!("Label bank ready: {} terms x {} dims", term_count, embedding_dim);

           Ok(Self {
               matrix,
               embedding_dim,
               term_count,
           })
       }

       /// Save label bank to disk for fast reload.
       pub fn save(&self, path: &Path) -> Result<(), PipelineError> {
           // Save as raw f32 binary for fast memory-mapped loading
           let bytes: Vec<u8> = self.matrix.iter()
               .flat_map(|f| f.to_le_bytes())
               .collect();
           std::fs::write(path, &bytes)?;
           tracing::info!("Saved label bank to {:?} ({:.1} MB)",
               path, bytes.len() as f64 / 1_000_000.0);
           Ok(())
       }

       /// Load label bank from disk.
       pub fn load(path: &Path, term_count: usize) -> Result<Self, PipelineError> {
           let bytes = std::fs::read(path)?;
           let embedding_dim = 768;
           let expected_len = term_count * embedding_dim * 4; // 4 bytes per f32

           if bytes.len() != expected_len {
               return Err(PipelineError::Model {
                   message: format!(
                       "Label bank size mismatch: expected {} bytes, got {}",
                       expected_len, bytes.len()
                   ),
               });
           }

           let matrix: Vec<f32> = bytes.chunks_exact(4)
               .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
               .collect();

           tracing::info!("Loaded label bank: {} terms from {:?}", term_count, path);

           Ok(Self {
               matrix,
               embedding_dim,
               term_count,
           })
       }

       /// Check if a saved label bank exists.
       pub fn exists(path: &Path) -> bool {
           path.exists()
       }

       /// Get the flat matrix for batch dot product.
       pub fn matrix(&self) -> &[f32] {
           &self.matrix
       }

       pub fn embedding_dim(&self) -> usize {
           self.embedding_dim
       }

       pub fn term_count(&self) -> usize {
           self.term_count
       }
   }
   ```

**Storage:**
```
~/.photon/taxonomy/
  label_bank.bin          # raw f32 binary (~240MB for 80K terms x 768 dims)
  label_bank.meta.json    # metadata: term count, encoding timestamp, version
```

**Acceptance Criteria:**
- [ ] All vocabulary terms encoded through text encoder
- [ ] Prompt templates averaged into single embedding per term
- [ ] Label bank saved to disk as raw binary
- [ ] Label bank loads from disk (instant)
- [ ] Subsequent runs skip encoding, load from cache

---

### 4a.4 Flat Brute-Force Scoring

**Goal:** Score each image against the full vocabulary via matrix multiplication.

**Steps:**

1. Create the tag scorer:
   ```rust
   // crates/photon-core/src/tagging/scorer.rs

   use crate::config::TaggingConfig;
   use crate::types::Tag;

   use super::label_bank::LabelBank;
   use super::vocabulary::Vocabulary;

   /// SigLIP learned scaling parameters (derived from combined model logits).
   /// These amplify tiny cosine differences into meaningful logits.
   /// See docs/completions/phase-4-text-encoder-spike.md for derivation.
   const LOGIT_SCALE: f32 = 117.33;
   const LOGIT_BIAS: f32 = -12.93;

   pub struct TagScorer {
       vocabulary: Vocabulary,
       label_bank: LabelBank,
       config: TaggingConfig,
   }

   impl TagScorer {
       pub fn new(
           vocabulary: Vocabulary,
           label_bank: LabelBank,
           config: TaggingConfig,
       ) -> Self {
           Self { vocabulary, label_bank, config }
       }

       /// Convert cosine similarity to confidence via SigLIP's sigmoid scoring.
       /// logit = LOGIT_SCALE * cosine + LOGIT_BIAS, then sigmoid(logit).
       fn cosine_to_confidence(cosine: f32) -> f32 {
           let logit = LOGIT_SCALE * cosine + LOGIT_BIAS;
           1.0 / (1.0 + (-logit).exp())
       }

       /// Score an image embedding against the full vocabulary.
       /// Returns tags sorted by confidence, filtered by min_confidence, limited to max_tags.
       pub fn score(&self, image_embedding: &[f32]) -> Vec<Tag> {
           // Dot product: image (1 x 768) . vocabulary^T (768 x N) = (1 x N) scores
           // Both are L2-normalized, so dot product = cosine similarity
           let n = self.label_bank.term_count();
           let dim = self.label_bank.embedding_dim();
           let matrix = self.label_bank.matrix();

           let mut scores: Vec<(usize, f32)> = Vec::with_capacity(n);

           for i in 0..n {
               let offset = i * dim;
               let cosine: f32 = (0..dim)
                   .map(|j| image_embedding[j] * matrix[offset + j])
                   .sum();
               let confidence = Self::cosine_to_confidence(cosine);
               scores.push((i, confidence));
           }

           // Filter by min_confidence (now in [0, 1] range after sigmoid)
           let terms = self.vocabulary.all_terms();
           let mut tags: Vec<Tag> = scores
               .into_iter()
               .filter(|(_, confidence)| *confidence >= self.config.min_confidence)
               .map(|(idx, confidence)| {
                   let term = &terms[idx];
                   Tag {
                       name: term.display_name.clone(),
                       confidence,
                       category: term.category.clone(),
                   }
               })
               .collect();

           // Sort by confidence descending
           tags.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap());

           // Limit to max_tags
           tags.truncate(self.config.max_tags);

           tags
       }
   }
   ```

**Performance note:** For 80K terms x 768 dims, this is ~60M multiply-adds — roughly 1-5ms on modern CPUs. No need for BLAS or GPU for this operation.

**Acceptance Criteria:**
- [ ] Dot product computed correctly for all terms
- [ ] min_confidence threshold filters low scores
- [ ] Results sorted by confidence descending
- [ ] max_tags limit enforced
- [ ] Scoring completes in < 10ms

---

### 4a.5 Integrate into Pipeline

**Goal:** Wire vocabulary loading, label bank, and scoring into the image processor.

**Steps:**

1. Update `ImageProcessor` to include `TagScorer`:
   ```rust
   // In crates/photon-core/src/pipeline/processor.rs

   use crate::tagging::{TagScorer, Vocabulary};
   use crate::tagging::label_bank::LabelBank;
   use crate::tagging::text_encoder::SigLipTextEncoder;

   pub struct ImageProcessor {
       // ... existing fields ...
       embedding_engine: Option<Arc<EmbeddingEngine>>,
       tag_scorer: Option<Arc<TagScorer>>,
   }

   impl ImageProcessor {
       // NOTE: new() stays sync and infallible — same as Phases 2-3.
       // Tagging is loaded via load_tagging(), following the load_embedding() pattern.
       pub fn new(config: &Config) -> Self {
           Self {
               // ... existing fields ...
               tag_scorer: None,
           }
       }

       /// Load the tagging system (vocabulary + label bank + scorer).
       /// Call this before processing if you want tags in the output.
       ///
       /// On first run, this encodes all vocabulary terms through the text
       /// encoder and caches the label bank to disk (~2-3 min for 80K terms).
       /// Subsequent runs load the cached label bank instantly.
       ///
       /// Follows the same opt-in pattern as load_embedding().
       pub fn load_tagging(&mut self, config: &Config) -> Result<()> {
           let vocab_dir = config.vocabulary_dir();
           let taxonomy_dir = config.taxonomy_dir();
           let model_dir = config.model_dir();

           // Load vocabulary
           let vocabulary = Vocabulary::load(&vocab_dir)?;

           // Load or build label bank
           let label_bank_path = taxonomy_dir.join("label_bank.bin");
           let label_bank = if LabelBank::exists(&label_bank_path) {
               LabelBank::load(&label_bank_path, vocabulary.len())?
           } else {
               // First run: encode all terms (takes 2-3 min)
               let text_encoder = SigLipTextEncoder::new(&model_dir)?;
               let bank = LabelBank::encode_all(&vocabulary, &text_encoder, 64)?;
               std::fs::create_dir_all(&taxonomy_dir)?;
               bank.save(&label_bank_path)?;
               bank
           };

           self.tag_scorer = Some(Arc::new(
               TagScorer::new(vocabulary, label_bank, config.tagging.clone())
           ));
           Ok(())
       }

       /// Check whether the tagging system is loaded.
       pub fn has_tagging(&self) -> bool {
           self.tag_scorer.is_some()
       }
   }
   ```

   In `process_with_options()`, add the tagging step after embedding:
   ```rust
       // Generate tags using embedding (Phase 4)
       let tags = if !options.skip_tagging {
           match (&self.tag_scorer, &embedding) {
               (Some(scorer), embedding) if !embedding.is_empty() => scorer.score(embedding),
               _ => vec![],
           }
       } else {
           vec![]
       };
   ```

   Add `skip_tagging` to `ProcessOptions`:
   ```rust
   pub struct ProcessOptions {
       pub skip_thumbnail: bool,
       pub skip_perceptual_hash: bool,
       pub skip_embedding: bool,
       pub skip_tagging: bool,  // new
   }
   ```

2. Update config:
   ```rust
   // In config.rs — update TaggingConfig

   #[derive(Debug, Clone, Serialize, Deserialize)]
   pub struct TaggingConfig {
       pub enabled: bool,
       pub min_confidence: f32,
       pub max_tags: usize,
       pub vocabulary: VocabularyConfig,
   }

   #[derive(Debug, Clone, Serialize, Deserialize)]
   pub struct VocabularyConfig {
       pub dir: String,
   }

   impl Default for TaggingConfig {
       fn default() -> Self {
           Self {
               enabled: true,
               min_confidence: 0.25,
               max_tags: 15,
               vocabulary: VocabularyConfig::default(),
           }
       }
   }

   impl Default for VocabularyConfig {
       fn default() -> Self {
           Self {
               dir: "~/.photon/vocabulary".to_string(),
           }
       }
   }
   ```

3. Update `Tag` type:
   ```rust
   // In types.rs

   #[derive(Debug, Clone, Serialize, Deserialize)]
   pub struct Tag {
       pub name: String,
       pub confidence: f32,
       #[serde(skip_serializing_if = "Option::is_none")]
       pub category: Option<String>,
   }
   ```

**Acceptance Criteria:**
- [ ] `ImageProcessor::new()` remains sync and infallible (no API break)
- [ ] `load_tagging()` follows the `load_embedding()` opt-in pattern
- [ ] `photon process image.jpg` outputs tags when tagging is loaded
- [ ] Tags appear in JSON output with name + confidence
- [ ] Different images produce different tags
- [ ] First run encodes vocabulary and caches to disk
- [ ] Subsequent runs load cache instantly
- [ ] Tagging can be disabled via `--no-tagging` flag or `tagging.enabled = false`

---

### 4a.6 Tests

```rust
#[tokio::test]
async fn test_text_encoder_produces_embeddings() {
    let encoder = SigLipTextEncoder::new(test_model_dir()).unwrap();
    let embedding = encoder.encode("a photograph of a dog").unwrap();
    assert_eq!(embedding.len(), 768);
    // Should be L2-normalized
    let norm: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
    assert!((norm - 1.0).abs() < 0.01);
}

#[tokio::test]
async fn test_batch_encoding() {
    let encoder = SigLipTextEncoder::new(test_model_dir()).unwrap();
    let texts = vec!["dog".to_string(), "cat".to_string(), "car".to_string()];
    let embeddings = encoder.encode_batch(&texts).unwrap();
    assert_eq!(embeddings.len(), 3);
    for emb in &embeddings {
        assert_eq!(emb.len(), 768);
    }
}

#[test]
fn test_vocabulary_loading() {
    let vocab = Vocabulary::load(test_vocab_dir()).unwrap();
    assert!(vocab.len() > 0);
    // Should have WordNet terms
    assert!(vocab.get("dog").is_some() || vocab.get("domestic_dog").is_some());
}

#[test]
fn test_prompt_templates() {
    let vocab = Vocabulary::load(test_vocab_dir()).unwrap();
    let term = vocab.get("dog").unwrap();
    let prompts = vocab.prompts_for(term);
    assert!(prompts.len() >= 2);
    assert!(prompts.contains(&"dog".to_string()));
    assert!(prompts.iter().any(|p| p.contains("photograph")));
}

#[tokio::test]
async fn test_scoring_produces_tags() {
    let config = Config::default();
    let mut processor = ImageProcessor::new(&config);
    processor.load_embedding(&config).unwrap();
    processor.load_tagging(&config).unwrap();
    let result = processor.process(Path::new("tests/fixtures/images/test.jpg")).await.unwrap();

    assert!(!result.tags.is_empty());
    for tag in &result.tags {
        assert!(tag.confidence >= 0.0 && tag.confidence <= 1.0);
        assert!(!tag.name.is_empty());
    }
}

#[tokio::test]
async fn test_min_confidence_filtering() {
    let mut config = Config::default();
    config.tagging.min_confidence = 0.9; // Very high threshold
    let mut processor = ImageProcessor::new(&config);
    processor.load_embedding(&config).unwrap();
    processor.load_tagging(&config).unwrap();
    let result = processor.process(Path::new("tests/fixtures/images/test.jpg")).await.unwrap();

    for tag in &result.tags {
        assert!(tag.confidence >= 0.9);
    }
}

#[tokio::test]
async fn test_max_tags_limit() {
    let mut config = Config::default();
    config.tagging.max_tags = 3;
    let mut processor = ImageProcessor::new(&config);
    processor.load_embedding(&config).unwrap();
    processor.load_tagging(&config).unwrap();
    let result = processor.process(Path::new("tests/fixtures/images/test.jpg")).await.unwrap();

    assert!(result.tags.len() <= 3);
}

#[test]
fn test_label_bank_save_load_roundtrip() {
    let vocab = Vocabulary::load(test_vocab_dir()).unwrap();
    let encoder = SigLipTextEncoder::new(test_model_dir()).unwrap();
    let bank = LabelBank::encode_all(&vocab, &encoder, 32).unwrap();

    let path = temp_dir().join("test_label_bank.bin");
    bank.save(&path).unwrap();

    let loaded = LabelBank::load(&path, vocab.len()).unwrap();
    assert_eq!(loaded.term_count(), bank.term_count());
}
```

---

## Phase 4b: Progressive Encoding

### Goal

Instead of blocking 2-3 minutes on first run, encode a seed set of ~2,000 terms in 2-3 seconds and process images immediately. Encode remaining terms in the background.

### Design

**First run:**
```
IMMEDIATE (2-3 seconds)
════════════════════════
  1. Load WordNet vocabulary (80K terms)
  2. Select initial set: 1K curated common visual terms + 1K random
  3. Encode those 2K through SigLIP text encoder
  4. Ready to process images

BACKGROUND (while processing images)
═════════════════════════════════════
  5. Encode remaining terms in chunks of 1K
  6. High-scoring terms trigger priority encoding of WordNet neighbors
  7. Save to label_bank.bin as chunks complete
  8. When fully encoded, single label_bank.bin replaces chunks
```

**Subsequent runs:**
```
  1. Load label_bank.bin from disk (instant)
  2. Ready immediately
```

### Implementation

1. Add a `SeedVocabulary` with ~1,000 curated common visual nouns (the most visually concrete terms from WordNet: animals, objects, foods, vehicles, etc.)
2. Add `PartialLabelBank` that can score against a subset of terms while encoding continues in background
3. Use `tokio::task::spawn_blocking` for background encoding (same pattern as Phase 3 embedding)
4. Store encoding progress in `label_bank.meta.json`

### Config

```toml
[tagging.vocabulary]
initial_sample_size = 2000
background_chunk_size = 1000
```

### Acceptance Criteria

- [ ] First run starts processing images within 3 seconds
- [ ] Tags are emitted against partial vocabulary immediately
- [ ] Background encoding completes all terms eventually
- [ ] Cached label bank used on subsequent runs
- [ ] No images fail due to incomplete vocabulary

---

## Phase 4c: Relevance Pruning (Three-Pool System)

### Goal

The vocabulary self-organizes to the user's library over time, reducing the active scoring set from 80K to 3-15K relevant terms.

### Three Pools

| Pool | Description | Scored per image? | Typical size |
|------|-------------|-------------------|--------------|
| **Active** | Matched at least once recently | Yes, every image | 3K-15K |
| **Warm** | Encoded but never/rarely matched | Periodically (every Nth image) | 10K-30K |
| **Cold** | Not yet encoded or irrelevant | No | Remainder |

### Per-Term Statistics

```json
{
  "term": "labrador_retriever",
  "times_above_threshold": 847,
  "avg_score_when_matched": 0.82,
  "last_matched": "2026-02-09",
  "relevance": 0.95,
  "pool": "active"
}
```

### Promotion/Demotion Rules

- Terms that score above `min_confidence` → stay in / move to **active**
- Active terms that haven't matched in `active_demotion_days` → demote to **warm**
- Warm terms checked every `warm_check_interval` images; if they match → promote to **active**
- Cold terms encoded in background; if they match warm check → promote

### Config

```toml
[tagging.vocabulary]
warm_check_interval = 100
cold_promotion_threshold = 0.3
active_demotion_days = 90
```

### Storage

```
~/.photon/taxonomy/
  relevance.json     # per-term statistics
  state.json         # pool assignments, encoding progress
```

### Acceptance Criteria

- [ ] Per-term statistics tracked and persisted
- [ ] Active pool shrinks over time for focused libraries
- [ ] Warm pool checked periodically
- [ ] Promotion/demotion rules work correctly
- [ ] Scoring time decreases as active pool narrows

---

## Phase 4d: Neighbor Expansion

### Goal

When a term enters the active pool, its WordNet neighbors (siblings, children) get priority encoding.

### Design

```
"labrador_retriever" becomes active
       │
       ▼
  WordNet lookup: siblings & children
    → "golden_retriever", "poodle", "german_shepherd",
      "chocolate_labrador", "yellow_labrador"
       │
       ▼
  Encode these immediately, add to warm pool
  (they'll promote to active if they start matching)
```

This means the vocabulary deepens automatically in areas where the user has images, without encoding the entire 80K upfront.

### Implementation

- When a term moves from warm/cold → active, look up its WordNet hypernym chain
- Find siblings (other children of the same parent) and direct children
- Move those from cold → warm (triggering encoding if needed)

### Acceptance Criteria

- [ ] Active term triggers neighbor lookup
- [ ] Neighbors are encoded and added to warm pool
- [ ] Duplicates are not re-encoded
- [ ] Neighbor expansion is bounded (max N per activation)

---

## Phase 4e: Hierarchy Deduplication

### Goal

Use WordNet hypernym chains to deduplicate tags and optionally display hierarchy paths.

### Design

When multiple tags share an ancestor chain, suppress the less specific ones:
```
Before dedup: "labrador" (0.87), "dog" (0.81), "animal" (0.72), "carpet" (0.74)
After dedup:  "labrador" (0.87), "carpet" (0.74)
              ("dog" and "animal" suppressed — ancestors of "labrador")
```

Optionally emit hierarchy path:
```json
{
  "name": "labrador retriever",
  "confidence": 0.87,
  "path": "animal > dog > labrador retriever"
}
```

### Implementation

1. After scoring and filtering, for each pair of remaining tags:
   - Check if one appears in the other's hypernym chain
   - If so, suppress the ancestor (keep the more specific descendant)
2. Optionally attach `path` field derived from hypernym chain

### Config

```toml
[tagging]
deduplicate_ancestors = true
show_paths = false
```

### Updated Tag Type

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tag {
    pub name: String,
    pub confidence: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}
```

### Acceptance Criteria

- [ ] Ancestor tags suppressed when descendant matches
- [ ] Path field populated when `show_paths = true`
- [ ] Deduplication can be disabled via config
- [ ] Non-WordNet terms (supplemental) unaffected by dedup

---

## Verification Checklist (Phase 4a — Gate for Phase 5)

Before moving to Phase 5, Phase 4a must pass:

- [ ] Vision encoder uses `pooler_output` (not `last_hidden_state`) for cross-modal alignment
- [ ] `photon models download` presents interactive model selection (224 / 384 / Both)
- [ ] Text encoder (fp32) + tokenizer downloaded alongside vision model(s)
- [ ] `photon models list` shows installed variants with default indicator
- [ ] `photon process image.jpg` outputs tags array
- [ ] `photon process image.jpg --quality high` uses 384 variant (if installed)
- [ ] Graceful fallback with warning when requested variant isn't installed
- [ ] Tags include name and confidence
- [ ] Confidence scores are in [0, 1] range
- [ ] Different images get different tags
- [ ] `min_confidence` config filters low-confidence tags
- [ ] `max_tags` config limits number of tags
- [ ] First run encodes vocabulary and caches label bank
- [ ] Subsequent runs load cached label bank instantly
- [ ] Tagging adds < 10ms per image (scoring only, excluding encoding)
- [ ] All tests pass

Phases 4b-4e can be implemented after Phase 5 (they're optimizations, not blocking).

---

## Files Created/Modified

```
crates/photon-core/src/
├── embedding/
│   ├── siglip.rs           # Updated: use pooler_output instead of last_hidden_state (4a.pre)
│   └── preprocess.rs       # Updated: parameterized image_size (was hardcoded 224)
├── tagging/
│   ├── mod.rs              # Module exports
│   ├── text_encoder.rs     # SigLIP text encoder wrapper
│   ├── vocabulary.rs       # WordNet vocabulary loader
│   ├── label_bank.rs       # Pre-computed term embeddings cache
│   └── scorer.rs           # Flat brute-force scoring
├── config.rs               # Updated: EmbeddingConfig.image_size, VocabularyConfig
├── types.rs                # Updated Tag struct
└── pipeline/
    └── processor.rs        # Updated with tag scoring + quality-aware model loading

crates/photon/src/cli/
├── models.rs               # Updated: interactive selector, dual-variant download, text encoder
└── process.rs              # Updated: --quality fast|high flag

~/.photon/models/
├── siglip-base-patch16/          # 224 variant (default)
│   └── visual.onnx               # ~350MB
├── siglip-base-patch16-384/      # 384 variant (optional)
│   └── visual.onnx               # ~350MB
├── text_model.onnx          # Shared text encoder, fp32 (~441MB)
└── tokenizer.json                # Shared tokenizer (~2MB)

~/.photon/vocabulary/
├── wordnet_nouns.txt             # Shipped with Photon (~2MB)
└── supplemental.txt              # Scenes, moods, styles (~500 terms)

~/.photon/taxonomy/
├── label_bank.bin                # Cached term embeddings (~240MB)
└── label_bank.meta.json
```

---

## Notes

- The text encoder uses the same `Mutex<Session>` pattern as the vision encoder in Phase 3
- For Phase 4a, encoding all 80K terms takes 2-3 minutes — acceptable for first-run experience
- Phase 4b will reduce first-run startup to 2-3 seconds with progressive encoding
- SigLIP's sigmoid loss gives better-calibrated confidence scores than CLIP's contrastive loss
- The vocabulary files need to be generated from WordNet and shipped with Photon (or downloaded on first run)
- Memory: 80K x 768 x 4 bytes = ~240MB for the label bank — acceptable for desktop, may need quantization for constrained devices
