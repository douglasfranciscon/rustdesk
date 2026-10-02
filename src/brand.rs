// CUSTOM BRANDING: the brand this build was made for, as the brand folder typed in
// "Run workflow" - empty on the default build (and for brremote, the default
// brand's own folder).
//
// The core needs it to register the machine in its brand's catalog on the API
// server (hbbs_http/catalog.rs); the UI reads its own copy in flutter/lib/brand.dart.
// res/brand/apply-brand.sh patches the line below before the build, the same way it
// patches brand.dart, so keep it in this exact shape.
pub const BRAND_FOLDER: &str = "";
