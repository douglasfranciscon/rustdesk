// CUSTOM BRANDING: the notices shown when the app opens on Windows - a message
// from the admin, an attendant's licence-expiry warning, and an offer to
// download a new version.
//
// All come from GET <api-server>/api/aviso-app, fetched once per run. It needs
// no login (the person opening the app is usually the client, who never logs
// in); an attendant's login is sent when there is one, and only then can the
// back answer with that attendant's licence warning - a login made after
// opening shows it on the next run. The back answers {"versao", "url_base",
// "mensagem", "licenca", "licenca_dias"}, strings "" when switched off; the
// global values live in the API server's App Settings. Anything else - no API
// server, a 404, a timeout, a body that is not that JSON - shows nothing: the
// app must open the same whether or not the back is there.
//
// The global message shows on every run. The licence warning does not: the
// back says whether there is one and how many days are left, and the app paces
// it - weekly from 30 days before expiry, daily in the last 7 (Douglas's call) -
// remembering in a local option the day it last showed one.
//
// The version is a date, "YYYY.MM.DD" (build_info.dart), and the announced one
// is a cutoff - the oldest build still acceptable, not the newest published:
// the app offers the download when its own date is older. Zero-padded, the
// text sorts like the calendar, so no parsing; a value in any other shape, on
// either side, offers nothing - a typo must not nag every client at once.
// See docs/0_GerarInstalador.md.

import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:flutter_hbb/brand.dart';
import 'package:flutter_hbb/build_info.dart';
import 'package:flutter_hbb/common.dart';
import 'package:flutter_hbb/models/platform_model.dart';
import 'package:flutter_hbb/utils/http_service.dart' as http;
import 'package:url_launcher/url_launcher.dart';

// The installer's name inside url_base. The single place that knows how the
// files are named: if they are ever renamed, this is the line to change (and
// it takes a new release to reach the installed apps).
const String _kWindowsInstaller = 'BRRemote-x86_64';
const String _kWindowsExtension = '.exe';

final _brVersionShape = RegExp(r'^\d{4}\.\d{2}\.\d{2}$');

// The licence warning's fields in the back's answer.
const String _kLicenceField = 'licenca';
const String _kLicenceDaysField = 'licenca_dias';
// The local option (BRRemote_local.toml, per Windows user) holding the day,
// "YYYY-MM-DD", the licence warning was last shown.
const String _kLicenceShownKey = 'br-licence-notice-last';

bool _checked = false;

/// Fetches the notice once per app run and shows what it asks for: the
/// message first, then the licence warning when due, then the download offer.
Future<void> checkAppNotice() async {
  if (!isWindows || _checked) return;
  _checked = true;
  final notice = await _fetchNotice();
  if (notice == null) return;

  final message = _field(notice, 'mensagem');
  final licence = _field(notice, _kLicenceField);
  final daysLeft = _days(notice, _kLicenceDaysField);
  final version = _field(notice, 'versao');
  final base = _field(notice, 'url_base');

  if (message.isNotEmpty) {
    await _showMessage('Aviso', message);
  }
  if (licence.isNotEmpty && daysLeft != null && _licenceDue(daysLeft)) {
    // Recorded before showing, so closing the app on the dialog still counts.
    await bind.mainSetLocalOption(key: _kLicenceShownKey, value: _isoDay(_today()));
    await _showMessage('Licença', licence);
  }
  // An unstamped (local) build has no date to compare: never nag it.
  if (base.isNotEmpty &&
      _brVersionShape.hasMatch(version) &&
      _brVersionShape.hasMatch(kBrVersion) &&
      kBrVersion.compareTo(version) < 0) {
    final download = await _askDownload(version);
    if (download == true) {
      await launchUrl(windowsInstallerUrl(base),
          mode: LaunchMode.externalApplication);
    }
  }
}

/// The back's answer, or null for anything that is not a 200 with a JSON object.
Future<Map<String, dynamic>?> _fetchNotice() async {
  try {
    final api = (await bind.mainGetApiServer()).trim();
    if (api.isEmpty) return null;
    final url = Uri.parse('$api/api/aviso-app').replace(queryParameters: {
      'plataforma': 'windows',
      'versao': kBrVersion,
      'marca': kBrandFolder,
    });
    // Logged in (an attendant): send the login, and the back may answer with
    // that attendant's licence-expiry warning in place of the global message.
    // A missing or stale login only falls back to the global answer, never to
    // an error. No login, no header at all.
    final loggedIn = bind.mainGetLocalOption(key: 'access_token').isNotEmpty;
    final resp = await http
        .get(url, headers: loggedIn ? getHttpHeaders() : null)
        .timeout(const Duration(seconds: 5));
    if (resp.statusCode != 200) return null;
    final body = jsonDecode(decode_http_response(resp));
    return body is Map<String, dynamic> ? body : null;
  } catch (e) {
    debugPrint('app notice: $e');
    return null;
  }
}

String _field(Map<String, dynamic> json, String key) {
  final v = json[key];
  return v is String ? v.trim() : '';
}

/// Days until expiry (0 = expires today), or null for anything that is not a
/// non-negative number - which shows no licence warning. The back sends -1
/// when there is nothing to warn about, so that 0 always means "today".
int? _days(Map<String, dynamic> json, String key) {
  final v = json[key];
  if (v is! num) return null;
  final days = v.toInt();
  return days >= 0 ? days : null;
}

/// Whether the licence warning is due: weekly while more than 7 days are left,
/// daily in the last 7. Dates are calendar days in UTC so a DST change cannot
/// shorten a day.
bool _licenceDue(int daysLeft) {
  final last = DateTime.tryParse(bind.mainGetLocalOption(key: _kLicenceShownKey));
  if (last == null) return true; // never shown, or an unreadable date
  final since =
      _today().difference(DateTime.utc(last.year, last.month, last.day)).inDays;
  if (since < 0) return true; // a stored date in the future: the clock moved
  return since >= (daysLeft <= 7 ? 1 : 7);
}

DateTime _today() {
  final now = DateTime.now();
  return DateTime.utc(now.year, now.month, now.day);
}

String _isoDay(DateTime d) => '${d.year.toString().padLeft(4, '0')}-'
    '${d.month.toString().padLeft(2, '0')}-${d.day.toString().padLeft(2, '0')}';

/// url_base + "BRRemote-x86_64" + "_<brand folder>" on a branded build + ".exe".
/// url_base may come with or without its trailing slash.
Uri windowsInstallerUrl(String base) {
  final folder = base.endsWith('/') ? base : '$base/';
  final brand = kBrandFolder.isEmpty ? '' : '_$kBrandFolder';
  final file = Uri.encodeComponent('$_kWindowsInstaller$brand$_kWindowsExtension');
  return Uri.parse(folder).resolve(file);
}

Future<void> _showMessage(String title, String message) async {
  await gFFI.dialogManager.show<bool>((setState, close, context) {
    return CustomAlertDialog(
      title: Text(title),
      content: SelectableText(message),
      actions: [dialogButton('OK', onPressed: close)],
      onSubmit: close,
      onCancel: close,
    );
  }, tag: 'app-notice-$title');
}

// `cutoff` is the minimum, not the newest: the folder may hold a later build,
// so the text names the user's version as outdated rather than promising which
// one the download brings.
Future<bool?> _askDownload(String cutoff) {
  return gFFI.dialogManager.show<bool>((setState, close, context) {
    submit() => close(true);
    return CustomAlertDialog(
      title: const Text('Nova versão disponível'),
      content: Text('A sua versão, de $kBrVersion, está desatualizada: a mínima '
          'recomendada é a de $cutoff.\n\nDeseja baixar a versão atual agora? '
          'O download abre no navegador.'),
      actions: [
        dialogButton('Não', onPressed: close, isOutline: true),
        dialogButton('Sim', onPressed: submit),
      ],
      onSubmit: submit,
      onCancel: close,
    );
  }, tag: 'app-notice-download');
}
