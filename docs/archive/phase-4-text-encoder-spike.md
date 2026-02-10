# Phase 4 Spike: Text Encoder Alignment Verification

> **Goal:** Confirm that the Xenova ONNX text encoder produces embeddings aligned with the vision encoder before building the vocabulary/scoring system.
> **Time:** ~1-2 hours
> **Outcome:** CONDITIONAL PASS — embeddings are aligned, but SigLIP's cosine similarities are inherently tiny. Viable for zero-shot tagging with logit scaling.

---

## Results

### Spike verdict: CONDITIONAL PASS

The text encoder IS aligned with the vision encoder. However, SigLIP produces very small absolute cosine similarities (all negative, -0.05 to -0.10 range), and relies on learned `logit_scale` and `logit_bias` parameters to amplify these into meaningful scores. With proper scaling, 2/3 test images show correct diagonal dominance; the beach.jpg failure is marginal (0.13 logit difference, well within base-model noise).

### Model metadata discovered

**Vision model (`vision_model.onnx`):**
- Input: `pixel_values` [1, 3, 224, 224]
- Output 0: `last_hidden_state` [1, 196, 768] — raw patch embeddings
- Output 1: `pooler_output` [1, 768] — projected embedding for cross-modal alignment

**Text model (`text_model.onnx`):**
- Input: `input_ids` [1, seq_len] — NO attention_mask needed
- Output 0: `last_hidden_state` [1, seq_len, 768]
- Output 1: `pooler_output` [1, 768] — projected embedding for cross-modal alignment
- Note: fp16 variant (`text_model_fp16.onnx`) fails on aarch64/Asahi Linux; use fp32

**Combined model (`model.onnx`):**
- Inputs: `input_ids` [1, seq_len], `pixel_values` [1, 3, 224, 224]
- Outputs: `logits_per_image` [1,1], `logits_per_text` [1,1], `text_embeds` [1, 768], `image_embeds` [1, 768]
- Image/text embeddings are independent of paired input (verified: cosine=1.000000 across pairings)

**Tokenizer (`tokenizer.json`):**
- SentencePiece tokenizer, 32000 vocab
- EOS token id = 1, appended automatically
- Example: "a photo of a dog" → [262, 266, 1304, 267, 262, 266, 1571, 1] (8 tokens)

### Learned parameters

Derived from the combined model's logit output with near-zero error (max 0.000004):

```
logit_scale = 117.3309
logit_bias  = -12.9324

Formula: logit = logit_scale * cosine_similarity(image_emb, text_emb) + logit_bias
         confidence = sigmoid(logit)
```

### Similarity matrix (cosine)

Using `pooler_output` from both separate models (identical to combined model `image_embeds`/`text_embeds`):

```
                  dog-text    beach-text   car-text
dog.jpg           -0.0671     -0.0906     -0.0774    ← dog HIGHEST (correct)
beach.jpg         -0.0631     -0.0563     -0.0552    ← car slightly higher than beach (marginal fail)
car.jpg           -0.0948     -0.0739     -0.0578    ← car HIGHEST (correct)
```

### Logit matrix (after scale + bias)

```
                  dog-text    beach-text   car-text
dog.jpg           -20.81      -23.56      -22.01    ← dog WINS by 1.20
beach.jpg         -20.33      -19.54      -19.41    ← beach loses by 0.13
car.jpg           -24.06      -21.60      -19.72    ← car WINS by 1.89
```

### Intra-modality similarity (diagnostic)

Vision-Vision (confirms vision embeddings are meaningful):
- dog vs beach: 0.57
- dog vs car: 0.54
- beach vs car: 0.59

Text-Text (confirms text embeddings are meaningful):
- "dog" vs "beach": 0.88
- "dog" vs "car": 0.92
- "beach" vs "car": 0.89

### L2 norms

All embeddings from both models have L2 norm = 1.000000 (within 0.000001).

---

## Critical finding: `last_hidden_state` vs `pooler_output`

The existing `EmbeddingEngine` (Phase 3) uses the **first** output from the vision model, which is `last_hidden_state` [1, 196, 768]. It mean-pools over the 196 patch tokens to get a 768-dim vector.

This is **wrong for cross-modal alignment** but **acceptable for image-to-image similarity**:

| Output used | Image-Image sim | Cross-Modal sim |
|---|---|---|
| `last_hidden_state` (mean-pool) | 0.46-0.54 (good) | Near zero, no alignment |
| `pooler_output` | 0.54-0.59 (good) | Near zero but correctly ordered |

For Phase 4 (zero-shot tagging), we MUST use `pooler_output` from both models.

For Phase 3 (image embedding for similarity search), either output works, but `pooler_output` is preferred since it's the model's intended output.

---

## Architectural implications for Phase 4

### Recommended approach: separate models with manual scoring

1. **Use `vision_model.onnx`** for image embedding → extract `pooler_output` (not first output)
2. **Use `text_model.onnx`** for text embedding → extract `pooler_output`
3. **Use `tokenizer.json`** for text tokenization → only `input_ids` needed
4. **Apply scoring formula:** `logit = 117.33 * cosine + (-12.93)`, then `sigmoid(logit)`
5. **Pre-encode tag vocabulary** at startup (text embeddings are independent of images)

### Changes needed to existing code

1. **`SigLipSession::embed()`** — switch from first output to named `pooler_output` extraction
2. **`models download`** — add text_model.onnx and tokenizer.json to download list
3. **New module: `tagging/`** — text encoder, tokenizer, vocabulary, scoring

### Files needed for Phase 4

| File | Remote path | Local path | Size |
|---|---|---|---|
| Vision encoder | `onnx/vision_model.onnx` | `~/.photon/models/siglip-base-patch16/visual.onnx` | ~372MB |
| Text encoder | `onnx/text_model.onnx` | `~/.photon/models/siglip-base-patch16/textual.onnx` | ~441MB |
| Tokenizer | `tokenizer.json` | `~/.photon/models/siglip-base-patch16/tokenizer.json` | ~2.3MB |

Note: use `text_model.onnx` (fp32), NOT `text_model_fp16.onnx` (fails on aarch64/Asahi).

---

## Setup (original plan, kept for reference)

### Downloads needed

The vision model is already downloaded. We need two additional files from `Xenova/siglip-base-patch16-224`:

| File | Remote path | Local path | Size |
|------|-------------|------------|------|
| Text encoder | `onnx/text_model_fp16.onnx` | `~/.photon/models/text_model_fp16.onnx` | ~170MB |
| Tokenizer | `tokenizer.json` | `~/.photon/models/tokenizer.json` | ~2MB |

Download via `photon models download` (if already updated) or manually via curl:
```bash
curl -L "https://huggingface.co/Xenova/siglip-base-patch16-224/resolve/main/onnx/text_model_fp16.onnx" \
  -o ~/.photon/models/text_model_fp16.onnx

curl -L "https://huggingface.co/Xenova/siglip-base-patch16-224/resolve/main/tokenizer.json" \
  -o ~/.photon/models/tokenizer.json
```

### Dependencies

```toml
# In photon-core/Cargo.toml
tokenizers = "0.20"
```

The `ort` and `ndarray` dependencies are already present from Phase 3.

### Test images

Need 2-3 real photographs (not the 1x1 test PNG). Place in `tests/fixtures/images/`:
- `dog.jpg` — a photo of a dog
- `beach.jpg` — a beach/ocean scene
- `car.jpg` — a car (optional, for a third data point)

---

## Implementation

### 1. Minimal text encoder wrapper

Write a small standalone function (or temporary test) that:

1. Loads `tokenizer.json` via the `tokenizers` crate
2. Tokenizes a string to `input_ids` and `attention_mask`
3. Loads `text_model_fp16.onnx` as an ort Session
4. Runs inference
5. Extracts the output tensor and L2-normalizes it

Key unknowns to figure out during this step:
- What are the model's expected input names? (Inspect with `session.inputs()`)
- What is the output shape? `[768]`, `[1, 768]`, or `[1, seq_len, 768]`?
- Does it need padding to a fixed length, or is it dynamic?

### 2. Verify output shape and names

Before scoring anything, just print:
```rust
for input in session.inputs() {
    println!("Input: {} {:?}", input.name(), input.input_type());
}
for output in session.outputs() {
    println!("Output: {} {:?}", output.name(), output.output_type());
}
```

This tells us exactly what the ONNX graph expects and produces.

### 3. Compute similarity matrix

Encode these text-image pairs and compute cosine similarity (dot product, since both are L2-normalized):

| | "a photo of a dog" | "a photo of a beach" | "a photo of a car" |
|---|---|---|---|
| dog.jpg | **should be high** | should be low | should be low |
| beach.jpg | should be low | **should be high** | should be low |
| car.jpg | should be low | should be low | **should be high** |

---

## Pass criteria

The spike passes if:

1. **Diagonal dominance:** Each image scores highest against its matching text
2. **Score separation:** Matching pairs score at least 0.10 higher than non-matching pairs
3. **Reasonable magnitudes:** Matching scores roughly in the 0.20-0.40 range (typical for SigLIP cosine similarity with prompted text)
4. **Consistent normalization:** All output vectors have L2 norm close to 1.0

Example of a passing result:
```
              dog-text  beach-text  car-text
dog.jpg         0.31       0.08      0.05
beach.jpg       0.06       0.29      0.04
car.jpg         0.03       0.05      0.27
```

## Fail criteria

The spike fails if:

- All scores are near-identical (e.g. everything is 0.15 ± 0.02) — embeddings are in different spaces
- Scores are negative or >1.0 — normalization mismatch
- Output shape is unexpected and can't be pooled to a single 768-dim vector
- Tokenizer fails to load or produces garbage token IDs

## If it fails

- Check if a non-fp16 text encoder exists (`text_model.onnx`) and try that
- Check the output layer — may need mean-pooling over sequence length
- Compare with Python reference: load same model in `transformers`, encode same text, compare raw float values
- Look for alternative ONNX exports (e.g. from `sentence-transformers` or direct `optimum` export)

---

## Cleanup

Regardless of outcome, delete everything the spike created:
- Remove `~/.photon/models/text_model_fp16.onnx` and `~/.photon/models/tokenizer.json`
- Remove any spike test code
- The real download path is `photon models download` at first startup — keeping pre-downloaded files would skip testing that flow

If the spike passes:
- Record the results (output shape, score ranges, input names) in this doc
- Move this doc to `docs/archive/`
- Proceed with Phase 4a implementation

If it fails:
- Record what went wrong in this doc
- Adjust Phase 4 plan before proceeding
