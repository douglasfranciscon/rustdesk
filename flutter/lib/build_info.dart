// CUSTOM BRANDING: this build's BR version - the date it was built, in Brasília
// time, as "YYYY.MM.DD" ("2026.10.01").
//
// It is what the new-version notice compares (app_notice.dart): the app offers
// a download when its date is older than the cutoff the API server announces.
// Zero-padded, the text sorts like the calendar, so comparing is plain string
// order. It is not crate::VERSION ("1.4.9", upstream's) nor the run number in
// the .exe's properties, and it is the same for every brand built that day.
//
// Empty here and in local builds, which therefore never get the notice. The CI
// stamps it in flutter-build.yml, next to the pubspec stamp. Keep the line in
// this exact shape - the CI's sed matches it.
const String kBrVersion = '';
