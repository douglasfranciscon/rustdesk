// CUSTOM BRANDING: what a build made with a brand folder changes besides artwork.
//
// The values committed here are the default BR Remote build. When a build picks a
// brand, res/brand/apply-brand.sh rewrites this whole file before compiling, so
// keep it to these two constants. See docs/1_MarcasAlternativas.md.

/// The brand folder this build was made with, as typed in "Run workflow".
/// Empty on the default build.
const String kBrandFolder = '';

/// The brand's site, without scheme: what the "Website" links show and open.
const String kBrandWebsite = 'www.brproj.com.br';
