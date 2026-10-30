Name:       brremote
Version:    1.4.9
Release:    0
Summary:    BR Remote
License:    GPL-3.0
URL:        https://www.brproj.com.br
Vendor:     BR PROJ
Requires:   gtk3 libxcb libXfixes alsa-lib libva2 pam gstreamer1-plugins-base
Recommends: libayatana-appindicator-gtk3 libxdo

# https://docs.fedoraproject.org/en-US/packaging-guidelines/Scriptlets/

%description
BR Remote, acesso remoto da BR PROJ.

%prep
# we have no source, so nothing here

%build
# we have no source, so nothing here

%global __python %{__python3}

%install
mkdir -p %{buildroot}/usr/bin/
mkdir -p %{buildroot}/usr/share/brremote/
mkdir -p %{buildroot}/usr/share/brremote/files/
mkdir -p %{buildroot}/usr/share/icons/hicolor/256x256/apps/
mkdir -p %{buildroot}/usr/share/icons/hicolor/scalable/apps/
install -m 755 $HBB/target/release/brremote %{buildroot}/usr/bin/brremote
install $HBB/libsciter-gtk.so %{buildroot}/usr/share/brremote/libsciter-gtk.so
install $HBB/res/brremote.service %{buildroot}/usr/share/brremote/files/
install $HBB/res/128x128@2x.png %{buildroot}/usr/share/icons/hicolor/256x256/apps/brremote.png
install $HBB/res/scalable.svg %{buildroot}/usr/share/icons/hicolor/scalable/apps/brremote.svg
install $HBB/res/brremote.desktop %{buildroot}/usr/share/brremote/files/
install $HBB/res/brremote-link.desktop %{buildroot}/usr/share/brremote/files/

%files
/usr/bin/brremote
/usr/share/brremote/libsciter-gtk.so
/usr/share/brremote/files/brremote.service
/usr/share/icons/hicolor/256x256/apps/brremote.png
/usr/share/icons/hicolor/scalable/apps/brremote.svg
/usr/share/brremote/files/brremote.desktop
/usr/share/brremote/files/brremote-link.desktop
/usr/share/brremote/files/__pycache__/*

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
    rm /usr/share/applications/brremote.desktop || true
    rm /usr/share/applications/brremote-link.desktop || true
    update-desktop-database
  ;;
  1)
    # for upgrade
  ;;
esac
