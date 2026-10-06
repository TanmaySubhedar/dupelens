# 0005: How the "keep" recommendation is chosen

- **Status:** Accepted; items marked ⚠️ **need confirmation** from the maintainer
- **Date:** 2026-10-05

## Context
The PRD lists ranking criteria: highest resolution, largest dimensions, better quality,
richest EXIF, earliest original creation time, favourite flag. Several are open to
interpretation.

## Decision
Criteria are compared **in the PRD's order**; the first one that differs decides
(`core/src/recommend.rs`):
1. Resolution = pixel count.
2. Dimensions = longest side (tie-breaker).
3. Quality = sharpness (edge strength on a normalised thumbnail), **bucketed (~19%)** so
   tiny differences don't count. Buckets keep the comparison consistent (transitive).
4. EXIF richness = number of EXIF tags.
5. Earliest EXIF capture date (a known date beats an unknown one).
6. ⚠️ Favourite = EXIF/XMP star rating **≥ 4**.
7. Full tie (e.g. byte-identical copies): oldest file on disk, then shortest path.

The UI shows a one-line reason, explaining the criterion that beat the best copy with
*different content* (identical twins can't explain anything).

## Open questions
- ⚠️ Favourite is last in PRD order, so it only breaks exact ties. Should a favourite
  **always** be kept instead?
- ⚠️ Batch-action meaning: *Restore Group* resets one group to the recommendation; *Keep
  Recommended* and *Clear Selection* apply to all groups. Confirm this matches intent.
