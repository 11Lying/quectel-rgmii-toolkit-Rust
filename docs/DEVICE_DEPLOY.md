# ImmortalWrt 部署

1. 由 ImmortalWrt netifd 配置 QMI 接口，确认 `qmi_wwan`、`/dev/cdc-wdmX` 和 `wwan0` 正常；SimpleAdmin 不创建、删除或重启这些资源。
2. 安装 `simpleadmin_*.ipk`。
3. 创建 `/etc/simpleadmin/auth`，权限设为 0600，不使用 `admin:admin`。
4. 编辑 `/etc/config/simpleadmin`，默认绑定 LAN 地址 `192.168.1.1` 和端口 `8080`。
5. 执行 `/etc/init.d/simpleadmin enable` 与 `start`。
6. 从 LAN 浏览器访问 `http://192.168.1.1:8080`。

故障排查只检查 procd 日志、`ip` 状态、`qmi_wwan` 加载状态和 USB serial 枚举。不要用 SimpleAdmin 代替 netifd 拨号，也不要通过 WebUI 修改路由、DNS、防火墙、WAN 或 OpenClash。
