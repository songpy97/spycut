# SpyCut open-source presentation design

## Goal

Make the repository understandable to an international audience in one screen while preserving the project's technical honesty. The primary position is a privacy-first, delete-only editor for long recordings. Online courses are the lead use case; webinars, training, screencasts, lecture archives, publishable meetings, and professional pre-edit cleanup expand the audience without changing the product model.

## Message hierarchy

The English `README.md` is the default entry point and the Simplified Chinese README is one click away. The opening sequence is: a short promise, download/contribution actions, preview warning, real editor screenshot, the problem SpyCut solves, then concrete use cases. Architecture, export precision, development, and licensing follow after the product story so evaluators can verify the claims without making new users read implementation details first.

The core line is: “Cut what should not be there. Keep everything else exactly where it was.” It communicates the immutable timeline more clearly than a generic “video editor” label. Claims must remain grounded in shipped behavior: local processing, read-only sources, delete-interval persistence, join review, supervised FFmpeg, transactional export, and output validation.

## Visual and community assets

Repository screenshots come only from the built-in synthetic demo. The gallery shows the editor, join review, and export supervision states. English contribution and security guides, privacy-aware issue forms, and a pull-request checklist make international participation possible while reinforcing the project's data-protection boundaries. The current Simplified Chinese application UI is disclosed explicitly; English UI localization remains a roadmap item rather than a simulated feature.
