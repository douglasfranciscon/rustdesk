// CUSTOM BRANDING: the notice shown when the app opens on Windows - a message
// from the admin, and an offer to download a new version.
//
// Both come from GET <api-server>/api/aviso-app, which needs no login (the
// person opening the app is usually the client, who never logs in). The back
// answers {"versao", "url_base", "mensagem"}, each "" when switched off; the
// values live in the API server's App Settings. Anything else - no API server,
// a 404, a timeout, a body that is not that JSON - shows nothing: the app must
// open the same whether or not the back is there.
//
// The version is a date, "YYYY.MM.DD" (build_info.dart): the app offers the
// download when its own date is older than the announced one. Zero-padded, the
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

bool _checked = false;

/// Fetches the notice once per app run and shows what it asks for: the
/// message first, then the download offer.
Future<void> checkAppNotice() async {
  if (!isWindows || _checked) return;
  _checked = true;
  final notice = await _fetchNotice();
  if (notice == null) return;

  final message = _field(notice, 'mensagem');
  final version = _field(notice, 'versao');
  final base = _field(notice, 'url_base');

  if (message.isNotEmpty) {
    await _showMessage(message);
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
    final resp = await http.get(url).timeout(const Duration(seconds: 5));
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

/// url_base + "BRRemote-x86_64" + "_<brand folder>" on a branded build + ".exe".
/// url_base may come with or without its trailing slash.
Uri windowsInstallerUrl(String base) {
  final folder = base.endsWith('/') ? base : '$base/';
  final brand = kBrandFolder.isEmpty ? '' : '_$kBrandFolder';
  final file = Uri.encodeComponent('$_kWindowsInstaller$brand$_kWindowsExtension');
  return Uri.parse(folder).resolve(file);
}

Future<void> _showMessage(String message) async {
  await gFFI.dialogManager.show<bool>((setState, close, context) {
    return CustomAlertDialog(
      title: const Text('Aviso'),
      content: SelectableText(message),
      actions: [dialogButton('OK', onPressed: close)],
      onSubmit: close,
      onCancel: close,
    );
  }, tag: 'app-notice-message');
}

Future<bool?> _askDownload(String version) {
  return gFFI.dialogManager.show<bool>((setState, close, context) {
    submit() => close(true);
    return CustomAlertDialog(
      title: const Text('Nova versão disponível'),
      content: Text('Você está com a versão de $kBrVersion, e já existe a de '
          '$version.\n\nDeseja baixar agora? O download abre no navegador.'),
      actions: [
        dialogButton('Não', onPressed: close, isOutline: true),
        dialogButton('Sim', onPressed: submit),
      ],
      onSubmit: submit,
      onCancel: close,
    );
  }, tag: 'app-notice-download');
}
