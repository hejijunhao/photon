# Phase 4: Zero-Shot Tagging

> **Duration:** 1 week
> **Milestone:** `photon process image.jpg` outputs semantic tags with confidence scores

---

## Overview

This phase implements zero-shot image classification using SigLIP's text encoder. By comparing image embeddings against text embeddings of tag labels, we can classify images without training on specific categories. This enables flexible, extensible tagging that works with any vocabulary.

---

## Prerequisites

- Phase 3 completed (SigLIP image embeddings working)
- SigLIP text encoder model (textual.onnx) downloaded

---

## Background: Zero-Shot Classification

**How it works:**
1. Generate image embedding (done in Phase 3)
2. Generate text embeddings for candidate labels ("a photo of a beach", "a photo of a cat", etc.)
3. Compute cosine similarity between image and each text embedding
4. Labels with highest similarity become tags

**Why SigLIP is good for this:**
- Trained on image-text pairs, so embeddings are aligned
- Sigmoid loss (vs. contrastive) gives better calibrated confidences
- Text encoder produces embeddings in the same 768-dim space as images

---

## Implementation Tasks

### 4.1 SigLIP Text Encoder Integration

**Goal:** Load and run the SigLIP text encoder for generating text embeddings.

**Steps:**

1. Add tokenizer dependency:
   ```toml
   tokenizers = "0.20"
   ```

2. Create `crates/photon-core/src/tagging/mod.rs`:
   ```rust
   pub mod zero_shot;
   pub mod taxonomy;
   pub mod text_encoder;

   pub use zero_shot::ZeroShotTagger;
   pub use taxonomy::TagTaxonomy;
   ```

3. Create text encoder wrapper:
   ```rust
   // crates/photon-core/src/tagging/text_encoder.rs

   use ort::{Session, SessionBuilder, Value};
   use ndarray::Array2;
   use std::path::Path;
   use tokenizers::Tokenizer;

   use crate::error::PipelineError;

   pub struct SigLipTextEncoder {
       session: Session,
       tokenizer: Tokenizer,
       max_length: usize,
   }

   impl SigLipTextEncoder {
       pub fn new(model_path: &Path) -> Result<Self, PipelineError> {
           let textual_model_path = model_path.join("textual.onnx");
           let tokenizer_path = model_path.join("tokenizer.json");

           // Load ONNX model
           let session = SessionBuilder::new(&ort::Environment::default())?
               .with_optimization_level(ort::GraphOptimizationLevel::Level3)?
               .with_model_from_file(&textual_model_path)
               .map_err(|e| PipelineError::Tagging {
                   path: textual_model_path.clone(),
                   message: format!("Failed to load text encoder: {}", e),
               })?;

           // Load tokenizer
           let tokenizer = Tokenizer::from_file(&tokenizer_path)
               .map_err(|e| PipelineError::Tagging {
                   path: tokenizer_path.clone(),
                   message: format!("Failed to load tokenizer: {}", e),
               })?;

           Ok(Self {
               session,
               tokenizer,
               max_length: 64, // SigLIP default
           })
       }

       /// Encode a single text string to embedding
       pub fn encode(&self, text: &str) -> Result<Vec<f32>, PipelineError> {
           let embeddings = self.encode_batch(&[text.to_string()])?;
           Ok(embeddings.into_iter().next().unwrap())
       }

       /// Encode multiple texts to embeddings
       pub fn encode_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, PipelineError> {
           // Tokenize
           let encodings = self.tokenizer
               .encode_batch(texts.to_vec(), true)
               .map_err(|e| PipelineError::Tagging {
                   path: PathBuf::new(),
                   message: format!("Tokenization failed: {}", e),
               })?;

           // Prepare input tensors
           let batch_size = texts.len();
           let seq_len = self.max_length;

           let mut input_ids = vec![0i64; batch_size * seq_len];
           let mut attention_mask = vec![0i64; batch_size * seq_len];

           for (i, encoding) in encodings.iter().enumerate() {
               let ids = encoding.get_ids();
               let mask = encoding.get_attention_mask();

               for (j, &id) in ids.iter().take(seq_len).enumerate() {
                   input_ids[i * seq_len + j] = id as i64;
                   attention_mask[i * seq_len + j] = mask[j] as i64;
               }
           }

           // Create ONNX inputs
           let input_ids_tensor = Value::from_array(
               self.session.allocator(),
               &[batch_size as i64, seq_len as i64],
               &input_ids,
           )?;

           let attention_mask_tensor = Value::from_array(
               self.session.allocator(),
               &[batch_size as i64, seq_len as i64],
               &attention_mask,
           )?;

           // Run inference
           let outputs = self.session.run(vec![input_ids_tensor, attention_mask_tensor])?;

           // Extract embeddings
           let output = outputs.get(0).ok_or_else(|| PipelineError::Tagging {
               path: PathBuf::new(),
               message: "No output from text encoder".to_string(),
           })?;

           let flat_embeddings: Vec<f32> = output.try_extract()?.view().iter().copied().collect();
           let embedding_dim = flat_embeddings.len() / batch_size;

           // Split and normalize
           let embeddings: Vec<Vec<f32>> = flat_embeddings
               .chunks(embedding_dim)
               .map(|chunk| Self::normalize(chunk))
               .collect();

           Ok(embeddings)
       }

       fn normalize(embedding: &[f32]) -> Vec<f32> {
           let norm: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
           if norm > 0.0 {
               embedding.iter().map(|x| x / norm).collect()
           } else {
               embedding.to_vec()
           }
       }
   }
   ```

**Acceptance Criteria:**
- [ ] Text encoder loads successfully
- [ ] Tokenizer processes text correctly
- [ ] Text embeddings are 768-dim
- [ ] Batch encoding works
- [ ] Embeddings are normalized

---

### 4.2 Tag Taxonomy/Vocabulary

**Goal:** Define a comprehensive, categorized tag vocabulary for zero-shot classification.

**Steps:**

1. Create taxonomy structure:
   ```rust
   // crates/photon-core/src/tagging/taxonomy.rs

   use serde::{Deserialize, Serialize};
   use std::collections::HashMap;

   #[derive(Debug, Clone, Serialize, Deserialize)]
   pub struct TagDefinition {
       pub name: String,
       pub category: TagCategory,
       pub prompts: Vec<String>,  // Text prompts for this tag
   }

   #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
   #[serde(rename_all = "lowercase")]
   pub enum TagCategory {
       Object,    // Physical objects: car, dog, tree
       Scene,     // Environments: beach, city, forest
       Action,    // Activities: running, eating, playing
       Style,     // Artistic style: vintage, modern, minimalist
       Color,     // Dominant colors: blue, red, warm
       Weather,   // Weather conditions: sunny, rainy, cloudy
       Time,      // Time of day: sunset, night, morning
       Mood,      // Emotional tone: peaceful, dramatic, joyful
   }

   pub struct TagTaxonomy {
       tags: Vec<TagDefinition>,
       by_category: HashMap<TagCategory, Vec<usize>>,
   }

   impl TagTaxonomy {
       /// Create the default taxonomy
       pub fn default_taxonomy() -> Self {
           let tags = Self::build_default_tags();
           let mut by_category: HashMap<TagCategory, Vec<usize>> = HashMap::new();

           for (i, tag) in tags.iter().enumerate() {
               by_category.entry(tag.category).or_default().push(i);
           }

           Self { tags, by_category }
       }

       /// Load taxonomy from a JSON file (for customization)
       pub fn from_file(path: &std::path::Path) -> Result<Self, std::io::Error> {
           let content = std::fs::read_to_string(path)?;
           let tags: Vec<TagDefinition> = serde_json::from_str(&content)?;

           let mut by_category: HashMap<TagCategory, Vec<usize>> = HashMap::new();
           for (i, tag) in tags.iter().enumerate() {
               by_category.entry(tag.category).or_default().push(i);
           }

           Ok(Self { tags, by_category })
       }

       pub fn all_tags(&self) -> &[TagDefinition] {
           &self.tags
       }

       pub fn tags_in_category(&self, category: TagCategory) -> Vec<&TagDefinition> {
           self.by_category
               .get(&category)
               .map(|indices| indices.iter().map(|&i| &self.tags[i]).collect())
               .unwrap_or_default()
       }

       /// Get all unique prompts for embedding
       pub fn all_prompts(&self) -> Vec<(usize, String)> {
           self.tags
               .iter()
               .enumerate()
               .flat_map(|(i, tag)| {
                   tag.prompts.iter().map(move |p| (i, p.clone()))
               })
               .collect()
       }

       fn build_default_tags() -> Vec<TagDefinition> {
           vec![
               // Objects
               Self::tag("person", TagCategory::Object, &["a person", "a human", "someone"]),
               Self::tag("dog", TagCategory::Object, &["a dog", "a puppy", "a canine"]),
               Self::tag("cat", TagCategory::Object, &["a cat", "a kitten", "a feline"]),
               Self::tag("car", TagCategory::Object, &["a car", "an automobile", "a vehicle"]),
               Self::tag("building", TagCategory::Object, &["a building", "architecture"]),
               Self::tag("tree", TagCategory::Object, &["a tree", "trees", "foliage"]),
               Self::tag("flower", TagCategory::Object, &["a flower", "flowers", "blooming"]),
               Self::tag("food", TagCategory::Object, &["food", "a meal", "cuisine"]),
               Self::tag("phone", TagCategory::Object, &["a phone", "a smartphone", "mobile device"]),
               Self::tag("book", TagCategory::Object, &["a book", "books", "reading material"]),

               // Scenes
               Self::tag("beach", TagCategory::Scene, &["a beach", "sandy beach", "seaside"]),
               Self::tag("mountain", TagCategory::Scene, &["mountains", "mountain range", "peaks"]),
               Self::tag("city", TagCategory::Scene, &["a city", "urban area", "cityscape"]),
               Self::tag("forest", TagCategory::Scene, &["a forest", "woods", "woodland"]),
               Self::tag("ocean", TagCategory::Scene, &["the ocean", "the sea", "open water"]),
               Self::tag("park", TagCategory::Scene, &["a park", "garden", "green space"]),
               Self::tag("street", TagCategory::Scene, &["a street", "road", "pathway"]),
               Self::tag("interior", TagCategory::Scene, &["indoor", "interior", "inside a room"]),
               Self::tag("sky", TagCategory::Scene, &["the sky", "clouds", "atmosphere"]),

               // Actions
               Self::tag("walking", TagCategory::Action, &["walking", "strolling"]),
               Self::tag("running", TagCategory::Action, &["running", "jogging"]),
               Self::tag("sitting", TagCategory::Action, &["sitting", "seated"]),
               Self::tag("eating", TagCategory::Action, &["eating", "dining"]),
               Self::tag("playing", TagCategory::Action, &["playing", "having fun"]),
               Self::tag("working", TagCategory::Action, &["working", "at work"]),

               // Styles
               Self::tag("portrait", TagCategory::Style, &["a portrait", "portrait photo"]),
               Self::tag("landscape", TagCategory::Style, &["a landscape", "landscape photo"]),
               Self::tag("macro", TagCategory::Style, &["a macro photo", "close-up"]),
               Self::tag("vintage", TagCategory::Style, &["vintage style", "retro", "old-fashioned"]),
               Self::tag("modern", TagCategory::Style, &["modern style", "contemporary"]),
               Self::tag("minimalist", TagCategory::Style, &["minimalist", "simple", "clean"]),
               Self::tag("artistic", TagCategory::Style, &["artistic", "creative", "art"]),

               // Colors
               Self::tag("blue", TagCategory::Color, &["blue tones", "predominantly blue"]),
               Self::tag("green", TagCategory::Color, &["green tones", "predominantly green"]),
               Self::tag("red", TagCategory::Color, &["red tones", "predominantly red"]),
               Self::tag("warm", TagCategory::Color, &["warm colors", "warm tones", "golden"]),
               Self::tag("cool", TagCategory::Color, &["cool colors", "cool tones"]),
               Self::tag("black_and_white", TagCategory::Color, &["black and white", "monochrome", "grayscale"]),
               Self::tag("colorful", TagCategory::Color, &["colorful", "vibrant colors", "multicolored"]),

               // Weather
               Self::tag("sunny", TagCategory::Weather, &["sunny day", "bright sunshine"]),
               Self::tag("cloudy", TagCategory::Weather, &["cloudy sky", "overcast"]),
               Self::tag("rainy", TagCategory::Weather, &["rainy weather", "rain"]),
               Self::tag("snowy", TagCategory::Weather, &["snowy", "snow", "winter"]),
               Self::tag("foggy", TagCategory::Weather, &["foggy", "misty", "hazy"]),

               // Time
               Self::tag("sunrise", TagCategory::Time, &["sunrise", "dawn", "early morning"]),
               Self::tag("sunset", TagCategory::Time, &["sunset", "dusk", "golden hour"]),
               Self::tag("night", TagCategory::Time, &["nighttime", "at night", "dark"]),
               Self::tag("daytime", TagCategory::Time, &["daytime", "during the day", "daylight"]),

               // Mood
               Self::tag("peaceful", TagCategory::Mood, &["peaceful", "calm", "serene"]),
               Self::tag("dramatic", TagCategory::Mood, &["dramatic", "intense", "striking"]),
               Self::tag("joyful", TagCategory::Mood, &["joyful", "happy", "cheerful"]),
               Self::tag("melancholic", TagCategory::Mood, &["melancholic", "sad", "somber"]),
               Self::tag("mysterious", TagCategory::Mood, &["mysterious", "enigmatic"]),
           ]
       }

       fn tag(name: &str, category: TagCategory, prompts: &[&str]) -> TagDefinition {
           TagDefinition {
               name: name.to_string(),
               category,
               prompts: prompts.iter().map(|s| s.to_string()).collect(),
           }
       }
   }
   ```

2. Consider allowing user-defined taxonomy via config:
   ```toml
   [tagging]
   custom_taxonomy = "~/.photon/taxonomy.json"  # Optional override
   ```

**Acceptance Criteria:**
- [ ] Default taxonomy covers common categories
- [ ] Tags have multiple prompt variants for robustness
- [ ] Taxonomy is categorized for filtering
- [ ] Custom taxonomy loading works

---

### 4.3 Zero-Shot Classification

**Goal:** Implement the classification logic using cosine similarity.

**Steps:**

1. Create zero-shot classifier:
   ```rust
   // crates/photon-core/src/tagging/zero_shot.rs

   use std::path::Path;
   use std::sync::Arc;

   use crate::config::TaggingConfig;
   use crate::error::PipelineError;
   use crate::types::Tag;

   use super::taxonomy::{TagCategory, TagDefinition, TagTaxonomy};
   use super::text_encoder::SigLipTextEncoder;

   pub struct ZeroShotTagger {
       text_encoder: Arc<SigLipTextEncoder>,
       taxonomy: TagTaxonomy,
       config: TaggingConfig,
       // Pre-computed text embeddings for efficiency
       tag_embeddings: Vec<TagEmbedding>,
   }

   struct TagEmbedding {
       tag_index: usize,
       embedding: Vec<f32>,
   }

   impl ZeroShotTagger {
       /// Initialize tagger with pre-computed text embeddings
       pub fn new(
           model_path: &Path,
           config: TaggingConfig,
       ) -> Result<Self, PipelineError> {
           let text_encoder = Arc::new(SigLipTextEncoder::new(model_path)?);
           let taxonomy = TagTaxonomy::default_taxonomy();

           // Pre-compute text embeddings for all tag prompts
           tracing::info!("Pre-computing tag embeddings...");
           let prompts = taxonomy.all_prompts();
           let prompt_texts: Vec<String> = prompts.iter().map(|(_, p)| p.clone()).collect();

           let embeddings = text_encoder.encode_batch(&prompt_texts)?;

           let tag_embeddings: Vec<TagEmbedding> = prompts
               .iter()
               .zip(embeddings.into_iter())
               .map(|((tag_idx, _), emb)| TagEmbedding {
                   tag_index: *tag_idx,
                   embedding: emb,
               })
               .collect();

           tracing::info!("Pre-computed {} tag embeddings", tag_embeddings.len());

           Ok(Self {
               text_encoder,
               taxonomy,
               config,
               tag_embeddings,
           })
       }

       /// Classify an image using its embedding
       pub fn classify(&self, image_embedding: &[f32]) -> Vec<Tag> {
           if !self.config.zero_shot_enabled {
               return vec![];
           }

           // Compute similarity with all tag embeddings
           let mut tag_scores: Vec<(usize, f32)> = Vec::new();

           for tag_emb in &self.tag_embeddings {
               let similarity = Self::cosine_similarity(image_embedding, &tag_emb.embedding);
               tag_scores.push((tag_emb.tag_index, similarity));
           }

           // Group by tag and take maximum similarity across prompts
           let mut best_per_tag: std::collections::HashMap<usize, f32> = std::collections::HashMap::new();
           for (tag_idx, score) in tag_scores {
               let entry = best_per_tag.entry(tag_idx).or_insert(0.0);
               *entry = entry.max(score);
           }

           // Convert to tags with confidence
           let mut tags: Vec<Tag> = best_per_tag
               .into_iter()
               .filter(|(_, score)| *score >= self.config.min_confidence)
               .map(|(tag_idx, score)| {
                   let tag_def = &self.taxonomy.all_tags()[tag_idx];
                   Tag {
                       name: tag_def.name.clone(),
                       confidence: score,
                       category: Some(format!("{:?}", tag_def.category).to_lowercase()),
                   }
               })
               .collect();

           // Sort by confidence (descending)
           tags.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap());

           // Limit to max tags
           tags.truncate(self.config.max_tags);

           tags
       }

       /// Classify with specific category filter
       pub fn classify_category(
           &self,
           image_embedding: &[f32],
           category: TagCategory,
       ) -> Vec<Tag> {
           let category_tags = self.taxonomy.tags_in_category(category);
           let category_indices: std::collections::HashSet<usize> = category_tags
               .iter()
               .enumerate()
               .map(|(i, _)| i)
               .collect();

           self.classify(image_embedding)
               .into_iter()
               .filter(|tag| {
                   self.taxonomy.all_tags().iter()
                       .position(|t| t.name == tag.name)
                       .map(|idx| category_indices.contains(&idx))
                       .unwrap_or(false)
               })
               .collect()
       }

       fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
           // Both vectors are already normalized, so dot product = cosine similarity
           a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
       }
   }
   ```

**Acceptance Criteria:**
- [ ] Cosine similarity computed correctly
- [ ] Tags sorted by confidence
- [ ] Minimum confidence threshold works
- [ ] Maximum tags limit works
- [ ] Category filtering works

---

### 4.4 Confidence Scoring and Calibration

**Goal:** Ensure confidence scores are well-calibrated and meaningful.

**Steps:**

1. SigLIP naturally provides better-calibrated scores than CLIP, but we can improve:
   ```rust
   impl ZeroShotTagger {
       /// Apply sigmoid scaling to raw similarities for better calibration
       fn calibrate_score(raw_similarity: f32) -> f32 {
           // SigLIP similarities are typically in [-1, 1]
           // Map to [0, 1] with sigmoid-like transformation

           // Temperature scaling (adjust based on empirical testing)
           let temperature = 0.07;
           let scaled = raw_similarity / temperature;

           // Sigmoid to [0, 1]
           1.0 / (1.0 + (-scaled).exp())
       }
   }
   ```

2. Add calibration to the classification:
   ```rust
   pub fn classify(&self, image_embedding: &[f32]) -> Vec<Tag> {
       // ... (compute similarities)

       // Apply calibration
       let mut tag_scores: Vec<(usize, f32)> = tag_scores
           .into_iter()
           .map(|(idx, score)| (idx, Self::calibrate_score(score)))
           .collect();

       // ... (rest of classification)
   }
   ```

3. Consider adding top-k per category to ensure diversity:
   ```rust
   /// Ensure balanced representation across categories
   pub fn classify_balanced(&self, image_embedding: &[f32], per_category: usize) -> Vec<Tag> {
       let mut result = Vec::new();

       for category in [
           TagCategory::Object,
           TagCategory::Scene,
           TagCategory::Style,
           TagCategory::Color,
           TagCategory::Mood,
       ] {
           let category_tags = self.classify_category(image_embedding, category);
           result.extend(category_tags.into_iter().take(per_category));
       }

       result.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap());
       result
   }
   ```

**Acceptance Criteria:**
- [ ] Confidence scores are in [0, 1] range
- [ ] Scores are intuitive (high = confident match)
- [ ] Different images get different tag distributions
- [ ] Balanced option provides category diversity

---

### 4.5 Integrate Tagging into Pipeline

**Goal:** Wire zero-shot tagging into the image processing pipeline.

**Steps:**

1. Update `ImageProcessor`:
   ```rust
   // In crates/photon-core/src/pipeline/processor.rs

   use crate::tagging::ZeroShotTagger;

   pub struct ImageProcessor {
       decoder: ImageDecoder,
       thumbnail_gen: ThumbnailGenerator,
       validator: Validator,
       embedder: Arc<SigLipEmbedder>,
       tagger: Option<Arc<ZeroShotTagger>>,
   }

   impl ImageProcessor {
       pub async fn new(config: &Config) -> Result<Self> {
           let model_path = config.model_dir().join(&config.embedding.model);

           // ... (existing initialization)

           // Initialize tagger if enabled
           let tagger = if config.tagging.zero_shot_enabled {
               Some(Arc::new(ZeroShotTagger::new(
                   &model_path,
                   config.tagging.clone(),
               )?))
           } else {
               None
           };

           Ok(Self {
               decoder,
               thumbnail_gen,
               validator,
               embedder: Arc::new(embedder),
               tagger,
           })
       }

       pub async fn process(&self, path: &Path) -> Result<ProcessedImage> {
           // ... (existing pipeline stages)

           // Generate embedding
           let embedding = self.embedder.embed(&decoded.image)?;

           // Generate tags using embedding
           let tags = self.tagger
               .as_ref()
               .map(|t| t.classify(&embedding))
               .unwrap_or_default();

           // ... (rest of ProcessedImage construction)

           Ok(ProcessedImage {
               // ...
               embedding,
               tags,
               // ...
           })
       }
   }
   ```

**Acceptance Criteria:**
- [ ] Tags appear in JSON output
- [ ] Tags have name, confidence, category
- [ ] Tagging can be disabled via config
- [ ] No performance regression

---

### 4.6 Performance Optimization

**Goal:** Ensure tagging is fast by caching text embeddings.

**Steps:**

1. Text embeddings are pre-computed in `ZeroShotTagger::new()` (done in 4.3)

2. Consider lazy loading for large taxonomies:
   ```rust
   use once_cell::sync::Lazy;

   impl ZeroShotTagger {
       /// Get or compute tag embeddings lazily
       fn ensure_embeddings(&self) -> &[TagEmbedding] {
           // Already pre-computed in constructor
           &self.tag_embeddings
       }
   }
   ```

3. For very large taxonomies, consider batched similarity:
   ```rust
   use ndarray::{Array1, Array2};

   impl ZeroShotTagger {
       /// Batch similarity computation using matrix multiplication
       fn batch_similarities(&self, image_embedding: &[f32]) -> Vec<f32> {
           // Stack all tag embeddings into a matrix
           let n_tags = self.tag_embeddings.len();
           let dim = image_embedding.len();

           let tag_matrix = Array2::from_shape_fn(
               (n_tags, dim),
               |(i, j)| self.tag_embeddings[i].embedding[j]
           );

           let image_vec = Array1::from_vec(image_embedding.to_vec());

           // Matrix-vector multiplication for all similarities at once
           let similarities = tag_matrix.dot(&image_vec);
           similarities.to_vec()
       }
   }
   ```

**Acceptance Criteria:**
- [ ] Tagging adds < 10ms per image
- [ ] Memory usage is bounded
- [ ] Text embeddings computed only once at startup

---

## Integration Tests

```rust
#[tokio::test]
async fn test_zero_shot_tagging() {
    let config = Config::default();
    let processor = ImageProcessor::new(&config).await.unwrap();

    let result = processor.process(Path::new("tests/fixtures/images/beach.jpg")).await;
    let image = result.unwrap();

    assert!(!image.tags.is_empty());

    // Beach image should have beach-related tags
    let tag_names: Vec<&str> = image.tags.iter().map(|t| t.name.as_str()).collect();
    assert!(tag_names.contains(&"beach") || tag_names.contains(&"ocean"));
}

#[tokio::test]
async fn test_tag_confidence_range() {
    let config = Config::default();
    let processor = ImageProcessor::new(&config).await.unwrap();

    let result = processor.process(Path::new("tests/fixtures/images/test.jpg")).await;
    let image = result.unwrap();

    for tag in &image.tags {
        assert!(tag.confidence >= 0.0 && tag.confidence <= 1.0,
            "Tag confidence {} out of range", tag.confidence);
    }
}

#[tokio::test]
async fn test_tag_categories() {
    let config = Config::default();
    let processor = ImageProcessor::new(&config).await.unwrap();

    let result = processor.process(Path::new("tests/fixtures/images/test.jpg")).await;
    let image = result.unwrap();

    for tag in &image.tags {
        assert!(tag.category.is_some(), "Tag should have category");
        let category = tag.category.as_ref().unwrap();
        assert!(
            ["object", "scene", "action", "style", "color", "weather", "time", "mood"]
                .contains(&category.as_str())
        );
    }
}

#[test]
fn test_taxonomy_completeness() {
    let taxonomy = TagTaxonomy::default_taxonomy();
    let tags = taxonomy.all_tags();

    assert!(tags.len() >= 50, "Taxonomy should have substantial coverage");

    // Check all categories have tags
    for category in [
        TagCategory::Object,
        TagCategory::Scene,
        TagCategory::Style,
        TagCategory::Color,
    ] {
        let category_tags = taxonomy.tags_in_category(category);
        assert!(!category_tags.is_empty(), "Category {:?} should have tags", category);
    }
}
```

---

## Verification Checklist

Before moving to Phase 5:

- [ ] `photon process image.jpg` outputs tags array
- [ ] Tags include name, confidence, and category
- [ ] Confidence scores are calibrated (0.0 to 1.0)
- [ ] Different images get different tags
- [ ] Beach images get "beach" or similar tags
- [ ] Portrait images get "person" or "portrait" tags
- [ ] `min_confidence` config filters low-confidence tags
- [ ] `max_tags` config limits number of tags
- [ ] Tagging adds minimal overhead (< 10ms)
- [ ] All integration tests pass

---

## Files Created/Modified

```
crates/photon-core/src/
├── tagging/
│   ├── mod.rs           # Module exports
│   ├── text_encoder.rs  # SigLIP text encoder
│   ├── taxonomy.rs      # Tag vocabulary
│   └── zero_shot.rs     # Classification logic
└── pipeline/
    └── processor.rs     # Updated with tagging

tests/
└── fixtures/
    └── images/
        ├── beach.jpg     # Beach scene for testing
        └── portrait.jpg  # Portrait for testing
```

---

## Notes

- Text embeddings are computed once at startup, not per image
- The taxonomy can be customized by providing a JSON file
- Consider adding confidence calibration based on empirical testing
- For production, monitor which tags are most/least useful
- The prompts ("a photo of a...") format matters for SigLIP performance
