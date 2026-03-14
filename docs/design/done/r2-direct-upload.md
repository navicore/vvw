# R2 Direct Upload via S3 API

## Intent

`wrangler r2 object put` is unreliable for large audio files — uploads hang, time out, or silently fail, especially on constrained connections (LTE/5G hotspot). Each upload is a blocking subprocess call with no progress visibility. Retries restart from scratch.

Replace the wrangler subprocess calls for R2 uploads with direct S3-compatible API calls. This gives us:
- **Streaming progress**: byte-level upload progress bars
- **Multipart upload**: large files chunked into resumable parts
- **Timeout control**: per-chunk timeouts instead of one monolithic upload
- **Keep wrangler for everything else**: Pages deploy, R2 delete, preview server

## Constraints

- Must not change the R2 bucket layout or key naming (`{album}/audio/{filename}`)
- Must not require new Cloudflare dashboard setup beyond generating an API token (R2 already has S3 API enabled by default)
- Must preserve the `.r2-uploaded` manifest for skip-if-unchanged behavior
- Wrangler stays for `pages deploy`, `r2 object delete`, and `pages dev` — only `r2 object put` is replaced
- Must set `content-type` on uploaded objects (currently hardcoded `audio/flac`)
- Out of scope: replacing wrangler for Pages deploy or R2 delete

## Approach

### S3-compatible API

R2 exposes an S3-compatible endpoint at `https://<account_id>.r2.cloudflarestorage.com`. Authentication uses standard AWS Signature V4 with an R2 API token (Access Key ID + Secret Access Key), generated in the Cloudflare dashboard under R2 > Manage API Tokens.

### Rust implementation

Use the `aws-sdk-s3` crate (or the lighter `rust-s3`) to call:
- `PutObject` for files under ~100 MiB (single request with progress callback)
- `CreateMultipartUpload` / `UploadPart` / `CompleteMultipartUpload` for larger files (resumable, per-chunk timeout)

Progress is reported by wrapping the file `Body` in a reader that counts bytes and updates a progress bar (e.g., `indicatif` crate, or a simple `\r`-overwriting line).

### Configuration

Credentials via environment variables (standard S3 convention):
- `AWS_ACCESS_KEY_ID` — R2 API token access key
- `AWS_SECRET_ACCESS_KEY` — R2 API token secret
- `R2_ACCOUNT_ID` or `CLOUDFLARE_ACCOUNT_ID` — needed for the endpoint URL

Fall back to wrangler if env vars are not set, so the existing workflow still works without S3 credentials configured.

### Content type detection

Replace the hardcoded `audio/flac` with extension-based detection:
- `.audio` / `.flac` → `audio/flac`
- `.jpg` / `.jpeg` → `image/jpeg`
- `.png` → `image/png`
- Default → `application/octet-stream`

This fixes the current bug where image files (cover art, background artwork) are uploaded with `content-type: audio/flac`.

## Domain Events

- **File upload started** — prints filename, size, progress bar (replaces current `Uploading {key}...`)
- **Chunk uploaded** — progress bar updates (new, not possible with wrangler)
- **File upload complete** — prints elapsed time, updates `.r2-uploaded` manifest (same as today)
- **Upload failed** — retries with exponential backoff per-chunk, not per-file (improvement over today)
- **Fallback to wrangler** — if S3 env vars not set, uses existing `r2_put` path with a warning

## Checkpoints

1. With S3 credentials set, `just upload-audio` uploads files with a visible progress bar
2. Upload completes successfully — files accessible at the same R2 public URL as before
3. `.r2-uploaded` manifest updated identically to wrangler path
4. Large file (>50 MiB) uses multipart upload — visible in chunk-by-chunk progress
5. Network interruption mid-upload — retries from the failed chunk, not from scratch
6. Without S3 credentials, falls back to wrangler with a note suggesting S3 setup
7. Image files get correct content-type (not `audio/flac`)
