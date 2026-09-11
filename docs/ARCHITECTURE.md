# 目标架构

```text
Browser
  -> existing SimpleAdmin WebUI / Modem AT Console
  -> Axum HTTP/WebSocket API
  -> existing modem business logic (`At`)
  -> UsbAtTransport
  -> /dev/ttyUSB*, /dev/ttyACM*, or /dev/serial/by-id/*
  -> RM502Q-AE

RM502Q-AE USB QMI -> cdc-wdmX / qmi_wwan / wwan0
  -> ImmortalWrt netifd -> route / DNS / firewall / OpenClash
```

这是连接方式移植，不是业务重写。`At`、parser、缓存、页面聚合、SMS、转发、锁频和 WebUI 的原有 modem 功能保留；USB transport 只负责 tty 读写、超时、响应终止、URC 和短信交互。

## 边界

- SimpleAdmin 只通过 AT 管理 modem control plane。
- ImmortalWrt 管理 QMI、netifd、路由、DNS、防火墙、OpenClash 与 `wwan0` 生命周期。
- `NetworkStatus` 仅运行固定的只读 `ip` 查询；不得拨号或修改网络配置。
- USB discovery 只列出 serial 候选；其 metadata 不构成 capability 或功能门禁。
- 不引入 Adapter/profile/capability 状态层。未实机验证不隐藏功能、不返回 capability 403。

## AT 与写操作

正常 modem AT 保持可用。写操作仍要求登录、POST、参数校验、明确确认和审计；手工 AT 兼容 API 与 Modem AT Console 保留。只拒绝已明确退休的 gateway/data-plane AT：`QCFG="usbnet"`、QETH、QMAP DHCP/DMZ/LANIP/MPDN、RGMII 与 PCIe。

SimpleAdmin 不提供 Linux shell、任意系统命令执行或路由器 PTY；这不限制 modem AT Console。

## 运行路径

- `/etc/simpleadmin/`：认证和持久业务配置。
- `/usr/share/simpleadmin/www/`：静态 WebUI。
- `/usr/bin/simpleadmin-httpd`、`/etc/init.d/simpleadmin`：ImmortalWrt 服务包。

`root-password-init` 不迁移：ImmortalWrt root 凭据由系统自身配置。