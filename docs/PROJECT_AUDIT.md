# SimpleAdmin 项目审计

基线为 v0.2.6（`1f3dbf1a965c3c564f46cc1ea630b23c7fd2f686`）。目标是在 H5000M/ImmortalWrt 上以 USB serial AT port 连接 RM502Q-AE；这是 transport 移植，不是业务重写。

## 保留范围

`At` 队列和缓存、parser、Dashboard、设备/网络信息、SIM、信号、CA、小区扫描与锁定、频段、APN/PDP、IMEI、重启、AT&F、SMS 收发/删除/转发、流量趋势、兼容 AT API 和 Modem AT Console 都保留。实机未知的 AT 返回只在硬件到位后驱动最小 parser 修正，不是隐藏或拒绝功能的理由。

## 平台边界

`UsbAtTransport` 负责 raw tty、响应、超时、URC 与 SMS 交互；`usb_discovery` 仅发现 `/dev/serial/by-id`、`ttyUSB*` 与 `ttyACM*`。ImmortalWrt 负责 QMI、cdc-wdm、qmi_wwan、wwan0、netifd、路由、DNS、防火墙和 OpenClash。SimpleAdmin 只能读取固定的主机网络状态，不拨号或修改网络配置。

Ping、TTL、RM520N 内部 CPU/RAM、内部 Linux shell/PTTY 和 gateway/data-plane 管理不移植。`QCFG="usbnet"`、QETH、QMAP DHCP/DMZ/LANIP/MPDN、RGMII、PCIe 活动控制已退休；历史 parser/fixtures 可继续兼容。

## 安全模型

没有 capability state、Adapter/profile 门禁或“未验证即禁用”。写操作保持认证、POST、严格参数校验、明确确认和审计。AT Console 仅执行 modem AT，不提供 Linux shell。