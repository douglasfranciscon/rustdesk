Name:       brremote
Version:    1.4.9
Release:    0
Summary:    BR Remote
License:    GPL-3.0
URL:        https://www.brproj.com.br
Vendor:     BR PROJ
Requires:   gtk3 libxcb libXfixes alsa-lib libva pam gstreamer1-plugins-base
Recommends: libayatana-appindicator-gtk3 libxdo
Provides:   libdesktop_drop_plugin.so()(64bit), libdesktop_multi_window_plugin.so()(64bit), libfile_selector_linux_plugin.so()(64bit), libflutter_custom_cursor_plugin.so()(64bit), libflutter_linux_gtk.so()(64bit), libscreen_retriever_plugin.so()(64bit), libtray_manager_plugin.so()(64bit), liburl_launcher_linux_plugin.so()(64bit), libwindow_manager_plugin.so()(64bit), libwindow_size_plugin.so()(64bit), libtexture_rgba_renderer_plugin.so()(64bit)

# https://docs.fedoraproject.org/en-US/packaging-guidelines/Scriptlets/

%description
BR Remote, acesso remoto da BR PROJ.

%prep
# we have no source, so nothing here

%build
# we have no source, so nothing here

# %global __python %{__python3}

%install

mkdir -p "%{buildroot}/usr/share/brremote" && cp -r ${HBB}/flutter/build/linux/x64/release/bundle/* -t "%{buildroot}/usr/share/brremote"
mkdir -p "%{buildroot}/usr/bin"
install -Dm 644 $HBB/res/brremote.service -t "%{buildroot}/usr/share/brremote/files"
install -Dm 644 $HBB/res/brremote.desktop -t "%{buildroot}/usr/share/brremote/files"
install -Dm 644 $HBB/res/brremote-link.desktop -t "%{buildroot}/usr/share/brremote/files"
install -Dm 644 $HBB/res/128x128@2x.png "%{buildroot}/usr/share/icons/hicolor/256x256/apps/brremote.png"
install -Dm 644 $HBB/res/scalable.svg "%{buildroot}/usr/share/icons/hicolor/scalable/apps/brremote.svg"

%files
/usr/share/brremote/*
/usr/share/brremote/files/brremote.service
/usr/share/icons/hicolor/256x256/apps/brremote.png
/usr/share/icons/hicolor/scalable/apps/brremote.svg
/usr/share/brremote/files/brremote.desktop
/usr/share/brremote/files/brremote-link.desktop

%changelog
# let's skip this for now

%pre
# can do something for centos7
case "$1" in
  1)
    # for install
  ;;
  2)
    # for upgrade
    systemctl stop brremote || true
  ;;
esac

%post
cp /usr/share/brremote/files/brremote.service /etc/systemd/system/brremote.service
cp /usr/share/brremote/files/brremote.desktop /usr/share/applications/
cp /usr/share/brremote/files/brremote-link.desktop /usr/share/applications/
ln -sf /usr/share/brremote/brremote /usr/bin/brremote
systemctl daemon-reload
systemctl enable brremote
systemctl start brremote
update-desktop-database

%preun
case "$1" in
  0)
    # for uninstall
    systemctl stop brremote || true
    systemctl disable brremote || true
    rm /etc/systemd/system/brremote.service || true
  ;;
  1)
    # for upgrade
  ;;
esac

%postun
case "$1" in
  0)
    # for uninstall
    rm /usr/bin/brremote || true
    rmdir /usr/lib/brremote || true
    rmdir /usr/local/brremote || true
    rmdir /usr/share/brremote || true
    rm /usr/share/applications/brremote.desktop || true
    rm /usr/share/applications/brremote-link.desktop || true
    update-desktop-database
  ;;
  1)
    # for upgrade
    rmdir /usr/lib/brremote || true
    rmdir /usr/local/brremote || true
  ;;
esac
