// CUSTOM BRANDING: what a build made with a brand folder changes besides artwork.
//
// The values committed here are the default BR Remote build. When a build picks a
// brand, res/brand/apply-brand.sh patches these lines before compiling, one by
// one, so keep each constant on its own line in this shape.
// See docs/1_MarcasAlternativas.md.

/// The brand folder this build was made with, as typed in "Run workflow".
/// Empty on the default build.
const String kBrandFolder = '';

/// The brand's site, without scheme: what the "Website" links show and open.
const String kBrandWebsite = 'www.brproj.com.br';

// The brand's palette, read by MyTheme (common.dart). A brand gives only its
// main colour (cor.txt); res/brand/brand_colors.py derives the other four.
const int kBrandColor = 0xFF3CA332; // the logo colour; the ID
const int kBrandAccent = 0xFF2F8F28; // buttons and highlights, white text on top
const int kBrandDark = 0xFF24761F; // brand text and icons on a light surface
const int kBrandCmId = 0xFF21790B; // the ID in the connection manager
const int kBrandGrayBg = 0xFFEFF1EC; // light surface tinted with the brand
