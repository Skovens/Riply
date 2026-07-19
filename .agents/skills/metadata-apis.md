---
name: metadata-apis
description: >
  Integration with TMDb v3 and OMDb APIs.
  Covers search endpoints, episode/season data retrieval, rate limiting,
  caching strategies, and confidence score calculation.
metadata:
  language: rust
  version: "2.0+"
  compatibility: reqwest, serde, cache
---

# Metadata APIs (TMDb & OMDb)

## General Principles
- **Rate Limiting:** Respect API rate limits. Implement token bucket algorithm.
- **Caching:** Cache responses locally to reduce API calls.
- **Error Handling:** Gracefully handle HTTP errors (429, 404, 500).

## TMDb v3
- **Endpoints:**
  - `search/multi`: Search for movies and TV shows.
  - `search/movie`: Search for movies.
  - `search/tv`: Search for TV shows.
  - `tv/{id}/season/{season_number}`: Get season details.
  - `tv/{id}/season/{season_number}/episode/{episode_number}`: Get episode details.
- **Rate Limiting:** 40 requests/minute. Use a token bucket with rate=40, capacity=40.
- **Caching:**
  - Store responses in a local JSON file.
  - TTL: 24 hours.
  - Key: SHA-256 of query parameters.

## OMDb
- **Usage:** Fallback when TMDb fails or for type detection (`Type: "series"`).
- **Rate Limiting:** 1000 requests/day. Track usage in a local file.
- **Endpoints:**
  - `?t={title}&y={year}`: Exact search.
  - `?s={title}`: Fuzzy search (returns list).

## Confidence Scoring
- **Movie:**
  - Exact Title + Year: 0.95+ confidence.
  - Title Match, Year ±1: 0.80-0.90 confidence.
  - Title Match, No Year: 0.70-0.80 confidence.
  - No Match: < 0.70 confidence (NEEDS_REVIEW).
- **TV:**
  - Exact SxxExx + Episode Title: 0.99 confidence.
  - Exact SxxExx, No Title: 0.92 confidence.
  - Season Only: 0.80 confidence.
  - No Match: < 0.80 confidence (NEEDS_REVIEW).

## Error Handling
- HTTP 429: Wait and retry with exponential backoff.
- HTTP 404: Log warning, proceed to next candidate.
- HTTP 500: Log error, retry once.
- Network Timeout: Log error, proceed to next candidate.