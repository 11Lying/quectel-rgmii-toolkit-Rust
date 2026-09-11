# ImmortalWrt ARM64 构建

目标设备必须使用 ImmortalWrt SDK 提供的 musl 工具链；普通 glibc ARM64 二进制不属于可部署产物。进入 SDK 后执行：

```sh
make package/simpleadmin/compile V=s
```

产物位于 `bin/packages/*/base/simpleadmin_*.ipk`。安装前确认 `uname -m` 为 `aarch64`，并检查 IPK 内包含 `/usr/bin/simpleadmin-httpd`、`/usr/share/simpleadmin/www/`、`/etc/config/simpleadmin` 和 `/etc/init.d/simpleadmin`。

认证文件由管理员在安装后创建：

```sh
mkdir -p /etc/simpleadmin
umask 077
printf '%s:%s\n' admin '<chosen-password>' > /etc/simpleadmin/auth
chmod 600 /etc/simpleadmin/auth
/etc/init.d/simpleadmin enable
/etc/init.d/simpleadmin start
```

SimpleAdmin 不拨号、不执行 `uqmi`、不调用 `ifup`/`ifdown`，数据面由 netifd 管理。
