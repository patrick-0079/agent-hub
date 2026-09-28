//! 密钥保险库：用系统能力加密存储 API Key，明文永不落库。
//!
//! Windows 使用 **DPAPI**（`CryptProtectData` / `CryptUnprotectData`）—— 密文与当前
//! 用户账号绑定，换用户或换机器都无法解密。这里直接 FFI 声明 crypt32，不引入额外依赖。
//!
//! 落盘格式：`vault.json`，键为「资源:字段」标识，值为 DPAPI 密文的十六进制。
//! 明文只在两个时刻存在：用户显式查看、以及渲染写入配置的瞬间。

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

#[cfg(windows)]
mod dpapi {
    use std::ffi::c_void;

    #[repr(C)]
    pub struct DataBlob {
        pub cb_data: u32,
        pub pb_data: *mut u8,
    }

    #[link(name = "crypt32")]
    extern "system" {
        fn CryptProtectData(
            data_in: *const DataBlob,
            description: *const u16,
            entropy: *const DataBlob,
            reserved: *mut c_void,
            prompt: *mut c_void,
            flags: u32,
            data_out: *mut DataBlob,
        ) -> i32;
        fn CryptUnprotectData(
            data_in: *const DataBlob,
            description: *mut *mut u16,
            entropy: *const DataBlob,
            reserved: *mut c_void,
            prompt: *mut c_void,
            flags: u32,
            data_out: *mut DataBlob,
        ) -> i32;
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn LocalFree(mem: *mut c_void) -> *mut c_void;
    }

    /// CRYPTPROTECT_UI_FORBIDDEN：不弹任何 UI（后台服务/无界面场景必须）
    const UI_FORBIDDEN: u32 = 0x1;

    pub fn protect(plain: &[u8]) -> Result<Vec<u8>, String> {
        let mut input = DataBlob {
            cb_data: plain.len() as u32,
            pb_data: plain.as_ptr() as *mut u8,
        };
        let mut output = DataBlob {
            cb_data: 0,
            pb_data: std::ptr::null_mut(),
        };
        let ok = unsafe {
            CryptProtectData(
                &mut input,
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                UI_FORBIDDEN,
                &mut output,
            )
        };
        if ok == 0 {
            return Err(format!(
                "DPAPI 加密失败（系统错误 {}）",
                std::io::Error::last_os_error()
            ));
        }
        let bytes =
            unsafe { std::slice::from_raw_parts(output.pb_data, output.cb_data as usize).to_vec() };
        unsafe { LocalFree(output.pb_data as *mut c_void) };
        Ok(bytes)
    }

    pub fn unprotect(cipher: &[u8]) -> Result<Vec<u8>, String> {
        let mut input = DataBlob {
            cb_data: cipher.len() as u32,
            pb_data: cipher.as_ptr() as *mut u8,
        };
        let mut output = DataBlob {
            cb_data: 0,
            pb_data: std::ptr::null_mut(),
        };
        let ok = unsafe {
            CryptUnprotectData(
                &mut input,
                std::ptr::null_mut(),
                std::ptr::null(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                UI_FORBIDDEN,
                &mut output,
            )
        };
        if ok == 0 {
            return Err(format!(
                "DPAPI 解密失败（系统错误 {}）—— 密文可能来自其它用户或其它机器",
                std::io::Error::last_os_error()
            ));
        }
        let bytes =
            unsafe { std::slice::from_raw_parts(output.pb_data, output.cb_data as usize).to_vec() };
        unsafe { LocalFree(output.pb_data as *mut c_void) };
        Ok(bytes)
    }
}

#[cfg(not(windows))]
mod dpapi {
    pub fn protect(_plain: &[u8]) -> Result<Vec<u8>, String> {
        Err("当前平台暂未实现系统级加密（Windows 使用 DPAPI）".to_string())
    }
    pub fn unprotect(_cipher: &[u8]) -> Result<Vec<u8>, String> {
        Err("当前平台暂未实现系统级加密（Windows 使用 DPAPI）".to_string())
    }
}

/* --------------------------------------------------------------- 十六进制 */

fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

fn from_hex(text: &str) -> Result<Vec<u8>, String> {
    if text.len() % 2 != 0 {
        return Err("密文长度非法".to_string());
    }
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).map_err(|e| format!("密文解析失败：{}", e)))
        .collect()
}

/* ---------------------------------------------------------------- 保险库 */

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct VaultFile {
    version: u32,
    /// 标识 → DPAPI 密文（hex）
    entries: BTreeMap<String, String>,
}

pub struct Vault {
    path: PathBuf,
    inner: Mutex<VaultFile>,
}

impl Vault {
    pub fn open(path: &Path) -> Self {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let inner = std::fs::read_to_string(path)
            .ok()
            .and_then(|text| serde_json::from_str::<VaultFile>(&text).ok())
            .unwrap_or(VaultFile {
                version: 1,
                entries: BTreeMap::new(),
            });
        Self {
            path: path.to_path_buf(),
            inner: Mutex::new(inner),
        }
    }

    fn persist(&self, file: &VaultFile) -> Result<(), String> {
        let text = serde_json::to_string_pretty(file).map_err(|e| e.to_string())?;
        // 原子写：先写临时文件再改名
        let temp = self.path.with_extension("json.tmp");
        std::fs::write(&temp, text).map_err(|e| format!("写入保险库失败：{}", e))?;
        std::fs::rename(&temp, &self.path).map_err(|e| format!("提交保险库失败：{}", e))
    }

    /// 写入一个密钥（自动加密）。空字符串表示删除。
    pub fn set(&self, id: &str, secret: &str) -> Result<(), String> {
        if secret.trim().is_empty() {
            return self.remove(id);
        }
        let cipher = dpapi::protect(secret.as_bytes())?;
        let mut file = self.inner.lock().map_err(|_| "保险库锁不可用")?;
        file.entries.insert(id.to_string(), to_hex(&cipher));
        file.version = 1;
        self.persist(&file)
    }

    /// 取出明文（只在显式查看或渲染写入时调用）
    pub fn get(&self, id: &str) -> Option<String> {
        let file = self.inner.lock().ok()?;
        let hex = file.entries.get(id)?;
        let cipher = from_hex(hex).ok()?;
        let plain = dpapi::unprotect(&cipher).ok()?;
        String::from_utf8(plain).ok()
    }

    pub fn remove(&self, id: &str) -> Result<(), String> {
        let mut file = self.inner.lock().map_err(|_| "保险库锁不可用")?;
        file.entries.remove(id);
        self.persist(&file)
    }

    pub fn has(&self, id: &str) -> bool {
        self.inner
            .lock()
            .map(|f| f.entries.contains_key(id))
            .unwrap_or(false)
    }

    pub fn ids(&self) -> Vec<String> {
        self.inner
            .lock()
            .map(|f| f.entries.keys().cloned().collect())
            .unwrap_or_default()
    }

    /// 只返回掩码（界面展示用，永不返回明文）
    pub fn masked(&self, id: &str) -> Option<String> {
        self.get(id).map(|plain| crate::util::mask_secret(&plain))
    }

    /// 检查密文是否可解密（用于「保险库是否与当前用户/机器匹配」的自检）
    pub fn verify(&self) -> Result<usize, String> {
        let ids = self.ids();
        let mut failed: Vec<String> = Vec::new();
        for id in &ids {
            if self.get(id).is_none() {
                failed.push(id.clone());
            }
        }
        if failed.is_empty() {
            Ok(ids.len())
        } else {
            Err(format!("以下密钥无法解密（可能来自其它用户或机器）：{}", failed.join("、")))
        }
    }
}