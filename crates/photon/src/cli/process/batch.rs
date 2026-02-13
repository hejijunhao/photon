//! Batch processing: directory traversal with progress, skip-existing, and streaming output.

use std::collections::HashSet;
use std::fs::File;
use std::io::BufWriter;
use std::path::PathBuf;

use photon_core::{DiscoveredFile, OutputRecord, OutputWriter, ProcessedImage};

use super::enrichment::{run_enrichment_collect, run_enrichment_stdout};
use super::types::OutputFormat;
use super::{ProcessArgs, ProcessContext};

/// Process a directory of images with progress tracking and optional LLM enrichment.
pub async fn process_batch(
    mut ctx: ProcessContext,
    args: &ProcessArgs,
    files: Vec<DiscoveredFile>,
) -> anyhow::Result<()> {
    // Load existing hashes for --skip-existing
    let existing_hashes = if args.skip_existing {
        load_existing_hashes(&args.output)?
    } else {
        HashSet::new()
    };
    if !existing_hashes.is_empty() {
        tracing::info!(
            "Loaded {} existing hashes from output file",
            existing_hashes.len()
        );
    }

    // Set up progress bar
    let total = files.len() as u64;
    let progress = create_progress_bar(total);

    let mut succeeded: u64 = 0;
    let mut failed: u64 = 0;
    let mut skipped: u64 = 0;
    let mut total_bytes: u64 = 0;
    let start_time = std::time::Instant::now();
    // Collected results: needed for JSON array output (format requires all items)
    // and for LLM enrichment (enricher needs image metadata). For JSONL-only
    // without LLM, results are streamed directly (lines 84-91, 94-100) and
    // this Vec stays empty.
    let mut results = Vec::new();

    // Stream JSONL directly to file (avoids collecting all results in memory)
    let stream_to_file = args.output.is_some() && matches!(args.format, OutputFormat::Jsonl);
    let mut file_writer = if stream_to_file {
        let output_path = args.output.as_ref().unwrap();
        let file = if args.skip_existing && output_path.exists() {
            std::fs::OpenOptions::new().append(true).open(output_path)?
        } else {
            File::create(output_path)?
        };
        Some(OutputWriter::new(
            BufWriter::new(file),
            ctx.output_format,
            false,
        ))
    } else {
        None
    };

    for file in &files {
        // Skip already-processed files
        if !existing_hashes.is_empty() {
            if let Ok(hash) = photon_core::Hasher::content_hash(&file.path) {
                if existing_hashes.contains(&hash) {
                    skipped += 1;
                    progress.inc(1);
                    continue;
                }
            }
        }

        match ctx
            .processor
            .process_with_options(&file.path, &ctx.options)
            .await
        {
            Ok(result) => {
                succeeded += 1;
                total_bytes += result.file_size;

                // Stream to stdout immediately (JSONL only)
                if matches!(args.format, OutputFormat::Jsonl) && args.output.is_none() {
                    if ctx.llm_enabled {
                        let record = OutputRecord::Core(Box::new(result.clone()));
                        println!("{}", serde_json::to_string(&record)?);
                    } else {
                        println!("{}", serde_json::to_string(&result)?);
                    }
                }

                // Stream to file immediately (JSONL only)
                if let Some(writer) = &mut file_writer {
                    if ctx.llm_enabled {
                        writer.write(&OutputRecord::Core(Box::new(result.clone())))?;
                    } else {
                        writer.write(&result)?;
                    }
                }

                // Collect only when needed:
                // - LLM: enricher requires image data
                // - JSON format: array wrapper requires all items
                if ctx.llm_enabled || matches!(args.format, OutputFormat::Json) {
                    results.push(result);
                }
            }
            Err(e) => {
                failed += 1;
                tracing::error!("Failed: {:?} - {}", file.path, e);
            }
        }

        // Update progress bar with rate
        progress.inc(1);
        let elapsed = start_time.elapsed().as_secs_f64();
        if elapsed > 0.0 {
            let processed = succeeded + failed;
            let rate = processed as f64 / elapsed;
            progress.set_message(format!("{:.1} img/sec", rate));
        }
    }

    // ── Post-loop output handling ──

    if stream_to_file {
        // JSONL file: core records already written in the loop

        if let Some(enricher) = ctx.enricher.take() {
            let (patches, _) = run_enrichment_collect(enricher, results).await?;
            if let Some(writer) = &mut file_writer {
                for record in &patches {
                    writer.write(record)?;
                }
            }
        }

        if let Some(writer) = &mut file_writer {
            writer.flush()?;
        }
        if let Some(output_path) = &args.output {
            tracing::info!("Output written to {:?}", output_path);
        }
    } else {
        // Non-streaming paths: JSON file output and stdout

        // Write batch results to file (JSON format — must collect for array wrapper)
        if let Some(output_path) = args.output.as_ref().filter(|_| !results.is_empty()) {
            // For JSON format with skip-existing, merge existing records — appending arrays is invalid JSON.
            let mut existing_records: Vec<OutputRecord> = Vec::new();
            if args.skip_existing && output_path.exists() {
                let content = std::fs::read_to_string(output_path)?;
                if let Ok(records) = serde_json::from_str::<Vec<OutputRecord>>(&content) {
                    existing_records = records;
                } else {
                    tracing::warn!(
                        "--skip-existing: failed to parse existing JSON output at {:?} — \
                         existing records will not be merged",
                        output_path
                    );
                }
            }
            let file = File::create(output_path)?;
            let mut writer = OutputWriter::new(BufWriter::new(file), ctx.output_format, false);

            if ctx.llm_enabled {
                // ── LLM enrichment to file ──
                // Run enrichment first, then consume results by move (zero clones).
                let mut all_records: Vec<OutputRecord> = existing_records;

                if let Some(enricher) = ctx.enricher.take() {
                    let (patches, returned_results) =
                        run_enrichment_collect(enricher, results).await?;
                    all_records.extend(
                        returned_results
                            .into_iter()
                            .map(|r| OutputRecord::Core(Box::new(r))),
                    );
                    all_records.extend(patches);
                } else {
                    all_records
                        .extend(results.into_iter().map(|r| OutputRecord::Core(Box::new(r))));
                }

                writer.write_all(&all_records)?;
            } else if !existing_records.is_empty() {
                let mut all_records = existing_records;
                all_records.extend(results.into_iter().map(|r| OutputRecord::Core(Box::new(r))));
                writer.write_all(&all_records)?;
            } else {
                writer.write_all(&results)?;
            }

            writer.flush()?;
            tracing::info!("Output written to {:?}", output_path);
        } else if !results.is_empty()
            && args.output.is_none()
            && matches!(args.format, OutputFormat::Json)
        {
            if ctx.llm_enabled {
                // JSON array to stdout — combine core + enrichment in single array.
                // Run enrichment first, then consume results by move (zero clones).
                if let Some(enricher) = ctx.enricher.take() {
                    let (patches, returned_results) =
                        run_enrichment_collect(enricher, results).await?;
                    let mut all_records: Vec<OutputRecord> = returned_results
                        .into_iter()
                        .map(|r| OutputRecord::Core(Box::new(r)))
                        .collect();
                    all_records.extend(patches);
                    println!("{}", serde_json::to_string_pretty(&all_records)?);
                } else {
                    let all_records: Vec<OutputRecord> = results
                        .into_iter()
                        .map(|r| OutputRecord::Core(Box::new(r)))
                        .collect();
                    println!("{}", serde_json::to_string_pretty(&all_records)?);
                }
            } else {
                // JSON array to stdout (non-LLM batch)
                println!("{}", serde_json::to_string_pretty(&results)?);
            }
        } else if matches!(args.format, OutputFormat::Jsonl)
            && ctx.llm_enabled
            && args.output.is_none()
        {
            // LLM enrichment for JSONL stdout streaming
            // (JSON handled in combined array above; file output handled in stream_to_file)
            if let Some(enricher) = ctx.enricher.take() {
                run_enrichment_stdout(enricher, &results, false).await?;
            }
        }
    }

    // Save relevance tracking data (if enabled)
    if let Err(e) = ctx.processor.save_relevance(&ctx.config) {
        tracing::warn!("Failed to save relevance data: {e}");
    }

    // Finish progress bar and show summary
    let elapsed = start_time.elapsed();
    let rate = if elapsed.as_secs_f64() > 0.0 {
        succeeded as f64 / elapsed.as_secs_f64()
    } else {
        0.0
    };

    progress.finish_and_clear();

    // Print formatted summary
    print_summary(succeeded, failed, skipped, total_bytes, elapsed, rate);

    Ok(())
}

/// Load existing content hashes from a JSONL/JSON output file for --skip-existing.
fn load_existing_hashes(output_path: &Option<PathBuf>) -> anyhow::Result<HashSet<String>> {
    let mut hashes = HashSet::new();

    let Some(path) = output_path else {
        return Ok(hashes);
    };

    if !path.exists() {
        return Ok(hashes);
    }

    let content = std::fs::read_to_string(path)?;

    // Try JSON array first (handles --format json output)
    if let Ok(records) = serde_json::from_str::<Vec<OutputRecord>>(&content) {
        for record in records {
            if let OutputRecord::Core(img) = record {
                hashes.insert(img.content_hash);
            }
        }
        return Ok(hashes);
    }
    if let Ok(images) = serde_json::from_str::<Vec<ProcessedImage>>(&content) {
        for image in images {
            hashes.insert(image.content_hash);
        }
        return Ok(hashes);
    }

    // Fall back to line-by-line JSONL parsing
    tracing::debug!("Output file is not a JSON array — trying JSONL line-by-line");
    let mut skipped_lines = 0u64;
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Ok(record) = serde_json::from_str::<OutputRecord>(line) {
            if let OutputRecord::Core(img) = record {
                hashes.insert(img.content_hash);
            }
            continue;
        }
        if let Ok(image) = serde_json::from_str::<ProcessedImage>(line) {
            hashes.insert(image.content_hash);
        } else {
            skipped_lines += 1;
        }
    }
    if skipped_lines > 0 {
        tracing::warn!(
            "--skip-existing: {skipped_lines} lines in output file could not be parsed — \
             those images will be reprocessed"
        );
    }

    Ok(hashes)
}

/// Create a progress bar for batch processing.
fn create_progress_bar(total: u64) -> indicatif::ProgressBar {
    use indicatif::{ProgressBar, ProgressStyle};

    let pb = ProgressBar::new(total);
    pb.set_style(
        ProgressStyle::default_bar()
            .template(
                "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({percent}%) {msg}",
            )
            .unwrap()
            .progress_chars("##-"),
    );
    pb.set_message("starting...");
    pb
}

/// Print a formatted summary table after batch processing.
fn print_summary(
    succeeded: u64,
    failed: u64,
    skipped: u64,
    total_bytes: u64,
    elapsed: std::time::Duration,
    rate: f64,
) {
    let total = succeeded + failed + skipped;
    let mb_processed = total_bytes as f64 / 1_000_000.0;
    let throughput = if elapsed.as_secs_f64() > 0.0 {
        mb_processed / elapsed.as_secs_f64()
    } else {
        0.0
    };

    eprintln!();
    eprintln!("  ====================================");
    eprintln!("               Summary");
    eprintln!("  ====================================");
    eprintln!("    Succeeded:    {:>8}", succeeded);
    if failed > 0 {
        eprintln!("    Failed:       {:>8}", failed);
    }
    if skipped > 0 {
        eprintln!("    Skipped:      {:>8}", skipped);
    }
    eprintln!("  ------------------------------------");
    eprintln!("    Total:        {:>8}", total);
    eprintln!("    Duration:     {:>7.1}s", elapsed.as_secs_f64());
    eprintln!("    Rate:         {:>7.1} img/sec", rate);
    eprintln!("    Throughput:   {:>7.1} MB/sec", throughput);
    eprintln!("  ====================================");
}

#[cfg(test)]
mod tests {
    use super::*;
    use photon_core::types::{EnrichmentPatch, ProcessedImage};
    use std::io::Write;

    fn sample_image(hash: &str) -> ProcessedImage {
        ProcessedImage {
            file_path: std::path::PathBuf::from("/test/image.jpg"),
            file_name: "image.jpg".to_string(),
            content_hash: hash.to_string(),
            width: 100,
            height: 100,
            format: "jpeg".to_string(),
            file_size: 1000,
            embedding: vec![],
            exif: None,
            tags: vec![],
            description: None,
            thumbnail: None,
            perceptual_hash: None,
        }
    }

    #[test]
    fn test_load_existing_hashes_json_array() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("output.json");

        let records = vec![
            OutputRecord::Core(Box::new(sample_image("hash_a"))),
            OutputRecord::Core(Box::new(sample_image("hash_b"))),
        ];
        let json = serde_json::to_string_pretty(&records).unwrap();
        std::fs::write(&path, &json).unwrap();

        let hashes = load_existing_hashes(&Some(path)).unwrap();
        assert_eq!(hashes.len(), 2);
        assert!(hashes.contains("hash_a"));
        assert!(hashes.contains("hash_b"));
    }

    #[test]
    fn test_load_existing_hashes_jsonl() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("output.jsonl");

        let mut f = std::fs::File::create(&path).unwrap();
        let img_a = sample_image("hash_c");
        let img_b = sample_image("hash_d");
        writeln!(f, "{}", serde_json::to_string(&img_a).unwrap()).unwrap();
        writeln!(f, "{}", serde_json::to_string(&img_b).unwrap()).unwrap();

        let hashes = load_existing_hashes(&Some(path)).unwrap();
        assert_eq!(hashes.len(), 2);
        assert!(hashes.contains("hash_c"));
        assert!(hashes.contains("hash_d"));
    }

    #[test]
    fn test_load_existing_hashes_empty_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("empty.json");
        std::fs::write(&path, "").unwrap();

        let hashes = load_existing_hashes(&Some(path)).unwrap();
        assert!(hashes.is_empty());
    }

    #[test]
    fn test_load_existing_hashes_mixed_records() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mixed.json");

        let records: Vec<OutputRecord> = vec![
            OutputRecord::Core(Box::new(sample_image("hash_e"))),
            OutputRecord::Enrichment(EnrichmentPatch {
                content_hash: "hash_e".to_string(),
                description: "A test image".to_string(),
                llm_model: "test".to_string(),
                llm_latency_ms: 100,
                llm_tokens: None,
            }),
            OutputRecord::Core(Box::new(sample_image("hash_f"))),
        ];
        let json = serde_json::to_string(&records).unwrap();
        std::fs::write(&path, &json).unwrap();

        let hashes = load_existing_hashes(&Some(path)).unwrap();
        assert_eq!(hashes.len(), 2);
        assert!(hashes.contains("hash_e"));
        assert!(hashes.contains("hash_f"));
    }

    #[test]
    fn test_load_existing_hashes_missing_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nonexistent.json");

        let hashes = load_existing_hashes(&Some(path)).unwrap();
        assert!(hashes.is_empty());
    }

    #[test]
    fn test_load_existing_hashes_warns_on_corrupt_jsonl() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("corrupt.jsonl");

        let mut f = std::fs::File::create(&path).unwrap();
        let img_a = sample_image("hash_ok_1");
        let img_b = sample_image("hash_ok_2");
        writeln!(f, "{}", serde_json::to_string(&img_a).unwrap()).unwrap();
        writeln!(f, "this is not valid json at all").unwrap();
        writeln!(f, "{}", serde_json::to_string(&img_b).unwrap()).unwrap();

        let hashes = load_existing_hashes(&Some(path)).unwrap();
        // Both valid hashes should be found; the corrupt line is skipped
        assert_eq!(hashes.len(), 2);
        assert!(hashes.contains("hash_ok_1"));
        assert!(hashes.contains("hash_ok_2"));
    }

    #[test]
    fn test_load_existing_hashes_none_output() {
        let hashes = load_existing_hashes(&None).unwrap();
        assert!(hashes.is_empty());
    }
}
