# RM502Q-AE / ImmortalWrt 移植计划

## 目标

以 v0.2.6 为业务基线，只替换 RM520N 内部 AP 的连接方式：现有 `At` 业务逻辑通过 USB serial AT port 连接 RM502Q-AE。原有 modem 功能默认保留；只有明确属于旧 gateway/data-plane、Ping、TTL、模块内部 Linux/PTY/shell/CPU-RAM 的部分移除。

## 已实施边界

- `UsbAtTransport` 提供 raw tty、响应终止、超时、短信 prompt、ME/SM 存储切换和 `+CMTI` URC。
- `At` 继续承担命令队列、缓存、parser、页面聚合和 mock 契约。
- USB discovery 只提供 `/dev/serial/by-id`、`ttyUSB*`、`ttyACM*` 候选；不进行 capability 判定。
- WebUI 与 API 保留 Dashboard、设备/网络信息、频段/小区锁定、SIM、SMS、转发、IMEI、重启、AT&F，以及 AT Console/兼容 AT 端点。
- 仅已退休的 QMAP/QETH/usbnet/RGMII/PCIe gateway/data-plane AT 被拒绝；普通 modem AT 不因未实机验证而被拒绝。
- QMI、qmi_wwan、cdc-wdm、wwan0、netifd、路由、DNS、防火墙与 OpenClash 均由 ImmortalWrt 管理。

## 验证与硬件收尾

1. 维持 `cargo fmt --check`、`cargo check`、`cargo test`、`git diff --check` 的回归检查。
2. 使用 mock/PTY 覆盖 USB transport、SMS、锁频、API 和 telemetry。
3. 设备到位后只采集 AT 返回并最小化修正 parser 或 transport；不引入 capability/Adapter 架构，不隐藏既有功能。
4. 验收 USB 重插、模块重启恢复、QMI/netifd 共存、SMS、扫描和所有保留写操作。