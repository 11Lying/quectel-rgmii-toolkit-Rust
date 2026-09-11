function simpleSettings() {
      return {
        showModal: false,
        showImeiModal: false,
        showResetAtModal: false,
        isRebooting: false,
        imei: "-",
        newImei: "-",
        isRebooted: true,
        language: SimpleAdmin.Lang ? SimpleAdmin.Lang.getCurrentLanguage() : "zh-CN",
        isSavingLanguage: false,
        languageSaveMessage: "",
        webUsername: "",
        webHttpPort: 80,
        webHttpEnabled: true,
        webuiCurrentPassword: "",
        isSavingWebui: false,
        webuiSaveMessage: "",
        webuiNewUrl: "",
        currentPassword: "",
        newPassword: "",
        confirmPassword: "",
        isSavingPassword: false,
        passwordSaveMessage: "",
        currentRootPassword: "",
        newRootPassword: "",
        confirmRootPassword: "",
        isSavingRootPassword: false,
        rootPasswordSaveMessage: "",
        rebootCountdownTimer: null,

        t(key) {
          return SimpleAdmin.Lang ? SimpleAdmin.Lang.t(key) : key;
        },

        fetchLanguageSetting() {
          if (!SimpleAdmin.Lang) return Promise.resolve(this.language);
          return SimpleAdmin.Lang.load().then((language) => {
            this.language = language;
            return language;
          });
        },

        saveLanguageSetting() {
          if (!SimpleAdmin.Lang) return;
          this.isSavingLanguage = true;
          this.languageSaveMessage = "";
          SimpleAdmin.Lang.setLanguage(this.language)
            .then((language) => {
              this.language = language;
              this.languageSaveMessage = this.t("已保存");
              setTimeout(() => {
                this.languageSaveMessage = "";
              }, 3000);
            })
            .catch((error) => {
              console.error("保存语言设置失败：", error);
              this.languageSaveMessage = this.t("保存失败");
            })
            .finally(() => {
              this.isSavingLanguage = false;
            });
        },

        async changeRootPassword() {
          if(this.isSavingRootPassword) return;
          this.rootPasswordSaveMessage = '';
          if(!this.currentRootPassword || !this.newRootPassword || this.newRootPassword !== this.confirmRootPassword) {
            this.rootPasswordSaveMessage = '请填写当前 root 密码，并确认两次新密码一致'; return;
          }
          this.isSavingRootPassword = true;
          const controller = new AbortController();
          const deadline = setTimeout(() => controller.abort(), 40000);
          try {
            const response = await fetch('/api/set_root_password', {method:'POST', signal:controller.signal, headers:{'Content-Type':'application/x-www-form-urlencoded'}, body:new URLSearchParams({current_password:this.currentRootPassword,new_password:this.newRootPassword,confirm_password:this.confirmRootPassword})});
            if(response.status === 401) {window.location.replace('/login.html');return;}
            const data = await response.json();
            if(!response.ok) throw new Error(data.error || '保存失败');
            this.currentRootPassword = this.newRootPassword = this.confirmRootPassword = '';
            window.location.replace('/login.html');
          } catch(error) {this.rootPasswordSaveMessage = error.name === 'AbortError' ? '请求超时，请确认设备密码与挂载状态' : error.message;}
          finally {clearTimeout(deadline);this.isSavingRootPassword=false;}
        },

        async fetchWebuiSetting() {
          try {
            const response = await fetch('/api/webui_settings', {cache:'no-store'});
            if (!response.ok) throw new Error(this.t("读取 WebUI 设置失败"));
            const data = await response.json();
            this.webUsername = data.username;
            this.webHttpPort = data.http_port;
            this.webHttpEnabled = data.http_enabled;
          } catch(error) { this.webuiSaveMessage = error.message; }
        },

        async saveWebuiPort() {
          if (this.isSavingWebui) return;
          this.webuiSaveMessage = ""; this.webuiNewUrl = "";
          if (!this.webuiCurrentPassword || !/^[1-9][0-9]{0,4}$/.test(String(this.webHttpPort)) || Number(this.webHttpPort) > 65535) {
            this.webuiSaveMessage = this.t("请填写当前 Web 密码和 1–65535 的端口"); return;
          }
          this.isSavingWebui = true;
          const controller = new AbortController();
          const deadline = setTimeout(() => controller.abort(), 40000);
          try {
            const response = await fetch('/api/set_webui_port', {method:'POST',signal:controller.signal,
              headers:{'Content-Type':'application/x-www-form-urlencoded'},
              body:new URLSearchParams({current_password:this.webuiCurrentPassword,http_port:String(this.webHttpPort)})});
            const data = await response.json();
            if (!response.ok) {
              const errors = {"current password incorrect":"当前密码不正确","HTTP port is already in use or unavailable":"端口已被占用或不可用，原配置保持不变"};
              throw new Error(this.t(errors[data.error] || data.error || "保存失败"));
            }
            this.webuiCurrentPassword = "";
            this.webuiSaveMessage = this.t(data.changed ? "端口已保存并生效，无需重启模块" : "端口未改变");
            if (data.warning) this.webuiSaveMessage += " · " + data.warning;
            if (data.changed) {
              if (["127.0.0.1","localhost","[::1]"].includes(window.location.hostname)) {
                this.webuiSaveMessage += " · " + this.t("通过 ADB 转发访问时，请在安装器重新打开管理页面");
              } else {
                const url = new URL(window.location.href);
                url.protocol = "http:"; url.port = String(data.http_port); url.pathname = "/login.html"; url.search = ""; url.hash = "";
                this.webuiNewUrl = url.href;
              }
            }
          } catch(error) { this.webuiSaveMessage = error.name === 'AbortError' ? this.t("请求超时，请重新检查当前端口") : error.message; }
          finally { clearTimeout(deadline); this.isSavingWebui = false; }
        },

        changeLoginPassword() {
          if (this.isSavingPassword) return;
          this.passwordSaveMessage = "";
          if (!this.currentPassword || !this.webUsername) {
            this.passwordSaveMessage = this.t("请输入当前密码和 Web 账号");
            return;
          }
          if (this.newPassword !== this.confirmPassword) {
            this.passwordSaveMessage = this.t("两次输入的新密码不一致");
            return;
          }

          this.isSavingPassword = true;
          return SimpleAdmin.Api.setPassword(this.currentPassword, this.newPassword, this.confirmPassword, this.webUsername)
            .then((res) => {
              return res.json().catch(() => ({})).then((data) => ({ res, data }));
            })
            .then(({ res, data }) => {
              if (!res.ok) {
                const error = data && data.error ? data.error : "";
                if (res.status === 403 || error === "current password incorrect") {
                  throw new Error("当前密码不正确");
                }
                if (error === "new password is empty") {
                  throw new Error("新密码不能为空");
                }
                if (error === "password confirmation mismatch") {
                  throw new Error("密码确认不一致");
                }
                throw new Error(error || "密码保存失败");
              }
              this.currentPassword = "";
              this.newPassword = "";
              this.confirmPassword = "";
              this.passwordSaveMessage = this.t("密码已保存，请使用新密码重新登录。");
              window.location.replace('/login.html');
            })
            .catch((error) => {
              console.error("保存登录密码失败：", error);
              this.passwordSaveMessage = this.t(error.message || "密码保存失败");
            })
            .finally(() => {
              this.isSavingPassword = false;
            });
        },

        closeModal() {
          this.showModal = false;
        },

        closeImeiModal() {
          this.showImeiModal = false;
        },

        closeResetAtModal() {
          this.showResetAtModal = false;
        },

        showRebootModal() {
          this.showModal = true;
        },

        startRebootCountdown(seconds = 40) {
          if (this.rebootCountdownTimer) {
            clearInterval(this.rebootCountdownTimer);
          }
          this.showModal = false;
          this.showImeiModal = false;
          this.isRebooting = true;
          this.countdown = seconds;
          this.isRebooted = false;  // 重启标志位，表示设备尚未重启完成

          // 进行倒计时
          this.rebootCountdownTimer = setInterval(() => {
            this.countdown--;
            if (this.countdown <= 0) {
              clearInterval(this.rebootCountdownTimer);
              this.rebootCountdownTimer = null;
              this.isRebooting = false;

              // 给设备一些时间重启后再执行初始化
              setTimeout(() => {
                this.isRebooted = true;  // 设置标志为已重启
                this.init();  // 重启后执行初始化（发送必要的 AT 命令）
              }, 5000);  // 延迟 5 秒，以确保设备完全重启
            }
          }, 1000);
        },

        handleRebootNotice(data) {
          if (!data || !(data.reboot || data.rebooting)) {
            return false;
          }
          const seconds = Number(data.rebootCountdownSeconds) || 40;
          this.startRebootCountdown(seconds);
          return true;
        },

        rebootDevice() {
          SimpleAdmin.Api.settingsData({ action: 'reboot', confirm: '1' });
          this.startRebootCountdown(40);
        },

        resetATCommands() {
          this.showResetAtModal = false;
          this.isLoading = true;
          SimpleAdmin.Api.settingsData({ action: 'reset_at', confirm: '1' })
            .catch((error) => {
              console.error('重置 AT 设置失败：', error);
              alert('重置 AT 设置失败，请检查调制解调器连接。');
            })
            .finally(() => {
              this.isLoading = false;
            });
        },


        openImeiModal() {
          const val = (this.newImei || '').trim();
          if (!val) {
            alert('没有提供新的 IMEI。');
            return;
          }
          if (val.length !== 15 || !/^\d+$/.test(val)) {
            alert('IMEI 无效');
            return;
          }
          if (this.imei !== '-' && val === this.imei) {
            alert('IMEI 与当前 IMEI 相同');
            return;
          }
          this.showImeiModal = true;
        },

        updateIMEI() {
          const val = (this.newImei || '').trim();
          this.showImeiModal = false;
          this.isLoading = true;
          SimpleAdmin.Api.settingsData({ action: 'set_imei', imei: val, confirm: '1' })
            .catch((error) => {
              console.info('设置 IMEI 后设备重启或连接断开，继续保持重启倒计时：', error);
            })
            .finally(() => {
              this.isLoading = false;
            });
          this.startRebootCountdown(40);
        },


        fetchCurrentSettings() {
          if (!this.isRebooted) {
            return;
          }
          SimpleAdmin.Api.settingsData({ action: 'status' })
            .then((data) => {
              const oldImei = this.imei;
              const currentImei = (data.imei || '').trim();
              if (/^\d{14,17}$/.test(currentImei)) {
                this.imei = currentImei;
                if (!this.newImei || this.newImei === '-' || this.newImei === oldImei) {
                  this.newImei = currentImei;
                }
              }
            })
            .catch((error) => {
              console.error("读取设备设置失败：", error);
            });
        },

        init() {
          if (!this.isRebooted) {
            return;  // 如果设备正在重启，跳过
          }

          this.fetchWebuiSetting();
          this.fetchLanguageSetting();
          this.fetchCurrentSettings();
        },
      };
    }

    if (window.SimpleAdminSpaMode) {
      (window.SimpleAdmin.Pages = window.SimpleAdmin.Pages || {}).settings = simpleSettings;
    } else {
      mountSimpleAdminVueApp(simpleSettings);
    }
