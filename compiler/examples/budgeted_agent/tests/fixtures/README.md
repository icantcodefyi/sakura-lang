# Synthetic cassette

Hand-authored request/response exchanges in the version 1 cassette format.
No live request, provider account, API key or recorded personal data was used.
Requests match method, URL, headers and body; secret headers are absent.
Tests install an empty scripted HTTP fallback, so a missing exchange fails offline.
