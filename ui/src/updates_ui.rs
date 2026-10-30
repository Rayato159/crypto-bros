use super::*;
#[component]
pub fn Updates() -> Element {
    let mut ctx = use_context::<Context>();
    let mut result = use_signal(|| Value::Null);
    let mut status = use_signal(|| json!({"message":"กดตรวจอัปเดตเพื่อดูรุ่นใหม่"}));
    let mut busy = use_signal(|| false);
    use_future(move || async move {
        loop {
            if let Ok(v) = invoke("update_status", json!({})).await {
                status.set(v);
            }
            gloo_timers::future::TimeoutFuture::new(500).await;
        }
    });
    let blocked = busy() || status()["busy"] == true;
    rsx! {section{class:"panel",h2{"อัปเดตโปรแกรม"}
        p{"รุ่นที่ใช้อยู่ {env!(\"CARGO_PKG_VERSION\")} · ตรวจลายเซ็นก่อนติดตั้ง พักบอตและสำรองข้อมูลให้ก่อนเปลี่ยนรุ่น"}
        div{class:"actions",
            button{class:"button primary",disabled:blocked,onclick:move |_|{
                busy.set(true);ctx.notice.set("กำลังตรวจอัปเดต…".into());
                spawn(async move {match invoke("check_update",json!({})).await {Ok(v)=>{ctx.notice.set(s(&v["message"]).into());result.set(v);},Err(e)=>ctx.notice.set(e)}busy.set(false);});
            },if blocked{"กำลังดำเนินการ…"}else{"ตรวจอัปเดต"}}
            button{class:"button secondary",onclick:move |_|{spawn(async move {if let Err(e)=invoke("open_releases",json!({})).await{ctx.notice.set(e);}});},"เปิดหน้า Release ↗"}
        }
        p{class:if status()["phase"]=="error"{"warning"}else{"update-message"},"{s(&status()[\"message\"])}"}
        if status()["phase"]=="downloading"{p{"ดาวน์โหลด {n(&status()[\"downloaded\"])/1048576.0:.1} MB"}
            if n(&status()["total"])>0.0{progress{max:"{n(&status()[\"total\"])}",value:"{n(&status()[\"downloaded\"])}"}}}
        if result()["available"]==true {
            h3{"มีรุ่น {s(&result()[\"version\"])}"}
            button{class:"button primary",disabled:blocked,onclick:move |_|{
                let version=s(&result()["version"]).to_string();busy.set(true);ctx.notice.set("กำลังดาวน์โหลด ตรวจลายเซ็น และเตรียมอัปเดต…".into());
                spawn(async move {match invoke("install_update",json!({"expectedVersion":version})).await{Ok(_)=>ctx.notice.set("เปิดตัวติดตั้งแล้ว หลังเสร็จเปิดแอปและเริ่มเฝ้าตลาดใหม่".into()),Err(e)=>ctx.notice.set(e)}busy.set(false);});
            },"ดาวน์โหลด พักบอต และติดตั้ง"}
        }
        p{class:"muted small","ถ้ามีธุรกรรมที่เซ็นแล้วค้างอยู่ จะยังติดตั้งไม่ได้ หลังอัปเดตแอปเริ่มในสถานะพัก ต้องกดเริ่มเฝ้าตลาดเอง"}
        p{class:"muted small","รุ่น 0.3.0 และเก่ากว่าต้องลง MSI รุ่นนี้เองหนึ่งครั้ง เพื่อเริ่มใช้ช่องอัปเดต ลายเซ็นอัปเดตเป็นคนละอย่างกับลายเซ็นผู้เผยแพร่ของ Windows"}
    }}
}
