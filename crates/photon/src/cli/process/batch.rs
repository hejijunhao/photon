//! Batch processing: directory traversal with progress, skip-existing, and streaming output.

use std::collections::HashSet;
use std::fs::File;
use std::io::BufWriter;
use std::path::PathBuf;

use photon_core::pipeline::DiscoveredFile;
use photon_core::types::{OutputRecord, ProcessedImage};
use photon_core::OutputWriter;

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
            if let Ok(hash) = photon_core::pipeline::Hasher::content_hash(&file.path) {
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
            let patches = run_enrichment_collect(enricher, results).await?;
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
            let file = if args.skip_existing && output_path.exists() {
                std::fs::OpenOptions::new().append(true).open(output_path)?
            } else {
                File::create(output_path)?
            };
            let mut writer = OutputWriter::new(BufWriter::new(file), ctx.output_format, false);

            if ctx.llm_enabled {
                // ── Phase 2: LLM enrichment to file (only if --llm) ──
                let mut all_records: Vec<OutputRecord> = results
                    .iter()
                    .map(|r| OutputRecord::Core(Box::new(r.clone())))
                    .collect();

                if let Some(enricher) = ctx.enricher.take() {
                    let patches = run_enrichment_collect(enricher, results.clone()).await?;
                    all_records.extend(patches);
                }

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
                // JSON array to stdout with OutputRecord::Core wrappers
                let core_records: Vec<OutputRecord> = results
                    .iter()
                    .map(|r| OutputRecord::Core(Box::new(r.clone())))
                    .collect();
                println!("{}", serde_json::to_string_pretty(&core_records)?);
            } else {
                // JSON array to stdout (non-LLM batch)
                println!("{}", serde_json::to_string_pretty(&results)?);
            }
        }

        // LLM enrichment for stdout streaming (JSON and JSONL)
        if ctx.llm_enabled && args.output.is_none() {
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
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        // Try parsing as OutputRecord first (dual-stream format)
        if let Ok(record) = serde_json::from_str::<OutputRecord>(line) {
            if let OutputRecord::Core(img) = record {
                hashes.insert(img.content_hash);
            }
            continue;
        }
        // Fall back to plain ProcessedImage
        if let Ok(image) = serde_json::from_str::<ProcessedImage>(line) {
            hashes.insert(image.content_hash);
        }
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
