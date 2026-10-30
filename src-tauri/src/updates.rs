use crate::{Backend, update_flow};
use serde_json::{Value, json};
use std::sync::atomic::Ordering;
use std::time::Duration;
use tauri_plugin_updater::UpdaterExt;

struct Permit<'a>(&'a Backend);
impl Drop for Permit<'_> {
    fn drop(&mut self) {
        self.0.update_busy.store(false, Ordering::Release);
    }
}
fn begin(backend: &Backend) -> Result<Permit<'_>, String> {
    backend
        .update_busy
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .map_err(|_| "กำลังตรวจหรือติดตั้งอัปเดตอยู่ กรุณารอ".to_string())?;
    Ok(Permit(backend))
}
fn progress(backend: &Backend, phase: &str, message: &str, downloaded: u64, total: Option<u64>) {
    if let Ok(mut value) = backend.update_status.lock() {
        *value = json!({"phase":phase,"message":message,"downloaded":downloaded,"total":total});
    }
}
pub fn status(backend: &Backend) -> Value {
    let mut v = backend
        .update_status
        .lock()
        .map(|v| v.clone())
        .unwrap_or_else(|_| json!({}));
    v["busy"] = json!(backend.update_busy.load(Ordering::Acquire));
    v
}
pub async fn check(app: &tauri::AppHandle, backend: &Backend) -> Result<Value, String> {
    let _permit = begin(backend)?;
    progress(backend, "checking", "กำลังตรวจรุ่นใหม่…", 0, None);
    let result:Result<Value,String>=async {
        let u=app.updater().map_err(|_|"ตั้งค่าช่องอัปเดตไม่สำเร็จ")?
            .check().await.map_err(|_|"ติดต่อช่องอัปเดตไม่ได้ ลองใหม่ภายหลังหรือเปิดหน้า Release")?;
        Ok(match u {
            Some(u)=>json!({"configured":true,"available":true,"version":u.version,"notes":u.body,"message":format!("มีรุ่น {} ให้ติดตั้ง",u.version)}),
            None=>json!({"configured":true,"available":false,"message":"ใช้รุ่นล่าสุดของช่องอัปเดตแล้ว"})
        })
    }.await;
    match &result {
        Ok(v) => progress(
            backend,
            "checked",
            v["message"].as_str().unwrap_or("ตรวจเสร็จแล้ว"),
            0,
            None,
        ),
        Err(e) => progress(backend, "error", e, 0, None),
    };
    result
}
pub async fn install(
    app: &tauri::AppHandle,
    backend: &Backend,
    expected_version: &str,
) -> Result<(), String> {
    let _permit = begin(backend)?;
    progress(backend, "checking", "กำลังตรวจรุ่นที่เลือกอีกครั้ง…", 0, None);
    let result: Result<(), String> = async {
        let update = app
            .updater()
            .map_err(|_| "ตั้งค่าช่องอัปเดตไม่สำเร็จ")?
            .check()
            .await
            .map_err(|_| "ตรวจอัปเดตไม่ได้ บอตยังไม่ถูกหยุด")?
            .ok_or("ไม่มีรุ่นใหม่ให้ติดตั้ง")?;
        if update.version != expected_version {
            return Err("รุ่นบนเซิร์ฟเวอร์เปลี่ยนแล้ว กดตรวจอัปเดตใหม่ก่อนติดตั้ง".into());
        }
        update_flow::install(
            || async {
                progress(
                    backend,
                    "downloading",
                    "กำลังดาวน์โหลดและตรวจลายเซ็น…",
                    0,
                    None,
                );
                let mut downloaded = 0u64;
                update
                    .download(
                        |chunk, total| {
                            downloaded = downloaded.saturating_add(chunk as u64);
                            progress(
                                backend,
                                "downloading",
                                "กำลังดาวน์โหลดและตรวจลายเซ็น…",
                                downloaded,
                                total,
                            );
                        },
                        || {},
                    )
                    .await
                    .map_err(|_| "ดาวน์โหลดหรือตรวจลายเซ็นไม่ผ่าน ยกเลิกก่อนหยุดบอต".into())
            },
            || async {
                progress(
                    backend,
                    "backup",
                    "ลายเซ็นผ่านแล้ว กำลังพักบอตและสำรองข้อมูล…",
                    0,
                    None,
                );
                backend
                    .command("prepare_update", json!({}))
                    .await
                    .map_err(|_| {
                        "ยังอัปเดตไม่ได้: ตรวจธุรกรรมค้างหรือการสำรองข้อมูล บอตอาจพักอยู่ ให้ตรวจสถานะก่อนเริ่มใหม่"
                            .to_string()
                    })?;
                Ok(())
            },
            || async {
                progress(
                    backend,
                    "stopping",
                    "สำรองข้อมูลแล้ว กำลังหยุดบริการก่อนติดตั้ง…",
                    0,
                    None,
                );
                backend
                    .command("shutdown", json!({}))
                    .await
                    .map_err(|_| "หยุดบริการไม่สำเร็จ ยกเลิกติดตั้ง ข้อมูลสำรองยังอยู่".to_string())?;
                for _ in 0..100 {
                    if !backend.data_dir.join("connection.json").exists() {
                        return Ok(());
                    }
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
                Err("บริการยังไม่หยุด ยกเลิกติดตั้ง ข้อมูลสำรองยังอยู่".into())
            },
            |bytes| {
                progress(
                    backend,
                    "installing",
                    "กำลังเปิดตัวติดตั้ง เมื่อเสร็จให้เปิดแอปใหม่ บอตจะเริ่มในสถานะพัก",
                    0,
                    None,
                );
                update
                    .install(bytes)
                    .map_err(|_| "เปิดตัวติดตั้งไม่สำเร็จ ข้อมูลสำรองยังอยู่ ให้ปิดแล้วเปิดแอปใหม่".into())
            },
        )
        .await
    }
    .await;
    if let Err(e) = &result {
        progress(backend, "error", e, 0, None);
    }
    result
}
pub fn open_release(app: &tauri::AppHandle) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    app.opener()
        .open_url(
            "https://github.com/Rayato159/crypto-bros/releases",
            None::<&str>,
        )
        .map_err(|_| "เปิดเบราว์เซอร์ไม่สำเร็จ".into())
}
