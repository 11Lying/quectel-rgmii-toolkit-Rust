include $(TOPDIR)/rules.mk

PKG_NAME:=simpleadmin
PKG_VERSION:=0.2.6
PKG_RELEASE:=1

include $(INCLUDE_DIR)/package.mk

# Build the Rust binary with the ImmortalWrt SDK toolchain.
define Package/simpleadmin
  SECTION:=admin
  CATEGORY:=Administration
  TITLE:=SimpleAdmin USB modem administration UI
  DEPENDS:=+libgcc
endef

define Package/simpleadmin/description
 SimpleAdmin serves the existing WebUI and reads modem/network state without managing netifd or QMI.
endef

define Build/Prepare
	$(CP) $(TOPDIR)/../../. $(PKG_BUILD_DIR)/
endef

define Package/simpleadmin/install
	$(INSTALL_DIR) $(1)/usr/bin $(1)/usr/share/simpleadmin/www $(1)/etc/config $(1)/etc/init.d $(1)/etc/simpleadmin
	$(INSTALL_BIN) $(PKG_BUILD_DIR)/target/aarch64-unknown-linux-musl/release/simpleadmin-httpd $(1)/usr/bin/simpleadmin-httpd
	$(CP) $(PKG_BUILD_DIR)/development/simpleadmin/www/. $(1)/usr/share/simpleadmin/www/
	$(INSTALL_CONF) ./files/etc/config/simpleadmin $(1)/etc/config/simpleadmin
	$(INSTALL_BIN) ./files/etc/init.d/simpleadmin $(1)/etc/init.d/simpleadmin
endef

$(eval $(call BuildPackage,simpleadmin))
