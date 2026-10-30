use super::*;
fn optional_money(v: &Value, scale: f64) -> String {
    v.as_f64()
        .map(|x| money(x / scale))
        .unwrap_or_else(|| "—".into())
}

#[component]
pub fn LiveDetails() -> Element {
    let ctx = use_context::<Context>();
    let data = (ctx.state)();
    let r = &data["wallet_readout"];
    let ind = &data["indicators"];
    let advice = &data["real_advice"];
    let last = items(&data["candles"])
        .last()
        .cloned()
        .unwrap_or(Value::Null);
    let fresh = data["source"] == "binance_solusdc"
        && data["market_error"].is_null()
        && r["fresh"] == true
        && data["engine_connected"] != false;
    rsx! {
        section{class:"panel spaced",h2{"SOL สำหรับค่าธรรมเนียม และยอดกระเป๋าจริง"}
            if r["available"]==true {
                if r["fresh"]!=true{p{class:"warning","ข้อมูลบางส่วนเก่าหรือยังตรวจไม่ครบ กดรีเฟรชก่อนใช้ตัดสินใจ"}}
                div{class:"metrics wallet-metrics",
                    div{class:"metric",div{class:"eyebrow","SOL ทั้งหมดในกระเป๋า"}div{class:"metric-value","{n(&r[\"native_sol\"])/1e9:.6}"}p{"≈ ฿{optional_money(&r[\"sol_satang\"],100.0)}"}}
                    div{class:"metric",div{class:"eyebrow","SOL ส่วนจ่ายค่าธรรมเนียม"}div{class:"metric-value","{n(&r[\"gas_sol\"])/1e9:.6}"}p{"≈ ฿{optional_money(&r[\"gas_satang\"],100.0)} · แยกจาก SOL ในไม้เทรด"}}
                    div{class:"metric",div{class:"eyebrow","มูลค่ากระเป๋ารวม"}div{class:"metric-value","฿{optional_money(&r[\"wallet_satang\"],100.0)}"}p{"≈ {optional_money(&r[\"wallet_usdc\"],1e6)} USDC · รวม SOL ที่เติมแยก"}}
                }
                div{class:"watch-row",span{"ค่าเครือข่ายที่บอตบันทึกสะสม"}strong{"{n(&r[\"network_fee_sol\"])/1e9:.9} SOL · {n(&r[\"network_fee_usdc\"])/1e6:.6} USDC · ฿{money(n(&r[\"network_fee_satang\"])/100.0)}"}}
                div{class:"watch-row",span{"เงินค่าเช่าบัญชีโทเคนสุทธิ (อาจได้คืน ไม่ใช่ค่าธรรมเนียม)"}strong{"{n(&r[\"net_rent_sol\"])/1e9:+.9} SOL"}}
                p{class:"muted small","ตรวจยอด: {chart::thai_time(n(&r[\"checked_at\"]) as i64)} · กัน SOL ขั้นต่ำ {n(&r[\"gas_floor\"])/1e9:.3} สำหรับค่าเครือข่าย"}
                if r["gas_low"]==true{p{class:"warning","SOL สำหรับค่าธรรมเนียมต่ำกว่าที่กันไว้ เติม SOL หรือรอรีเฟรชยอดก่อนซื้อเพิ่ม"}}
                if n(&r["wrapped_sol"])>0.0{p{class:"warning","มี wrapped SOL {n(&r[\"wrapped_sol\"])/1e9:.6} ซึ่งรวมในมูลค่ากระเป๋าแล้ว แต่ใช้จ่ายค่าเครือข่ายแทน native SOL ไม่ได้"}}
                p{class:"account-explainer","SOL ที่เติมเป็นเงินค่าธรรมเนียม ไม่เพิ่มวงเงินเทรดและไม่นับเป็นกำไร ยอดกระเป๋าบนเชนลดตามค่าธรรมเนียมไปแล้ว จึงไม่หักซ้ำ ค่า USDC/บาทของค่าเครือข่ายใช้ราคาตอนบันทึก เฉพาะรายการที่บอตบันทึกได้ ค่า swap และ slippage สะท้อนในยอดรับจริงแล้ว"}
            }else{p{"ยังไม่มีข้อมูลยอดกระเป๋าที่ตรวจแล้ว กดรีเฟรชราคา เรตบาท และยอดกระเป๋า"}}
        }
        section{class:"panel spaced",h2{"ข้อมูลที่บอตใช้ตัดสินใจตอนนี้"}
            p{class:if fresh{"muted"}else{"warning"},if fresh{"ข้อมูลล่าสุดที่ตรวจแล้ว"}else{"ข้อมูลยังไม่พร้อมหรืออาจเก่า อย่าใช้ค่านี้เป็นสัญญาณเข้าซื้อ"}}
            if !ind.is_null(){
                div{class:"metrics indicator-metrics",
                    div{class:"metric",div{class:"eyebrow","RSI (14)"}div{class:"metric-value","{n(&ind[\"rsi\"]):.2}"}p{if n(&ind["rsi"])>=70.0{"อยู่โซนสูง ≥ 70"}else if n(&ind["rsi"])<=30.0{"อยู่โซนต่ำ ≤ 30"}else{"อยู่ระหว่าง 30–70"}}}
                    div{class:"metric",div{class:"eyebrow","MACD (12,26)"}div{class:"metric-value","{n(&ind[\"macd\"]):+.4}"}p{"ผลต่างค่าเฉลี่ยเร็วกับช้า"}}
                    div{class:"metric",div{class:"eyebrow","Signal (9)"}div{class:"metric-value","{n(&ind[\"signal\"]):+.4}"}p{"ค่าเฉลี่ยของเส้น MACD"}}
                    div{class:"metric",div{class:"eyebrow","Histogram"}div{class:"metric-value","{n(&ind[\"histogram\"]):+.4}"}p{if n(&ind["histogram"])>0.0{"MACD อยู่เหนือ Signal"}else if n(&ind["histogram"])<0.0{"MACD อยู่ต่ำกว่า Signal"}else{"MACD เท่ากับ Signal"}}}
                }
                p{class:"muted small","แท่งล่าสุดเปิด {chart::thai_time(n(&last[\"time\"]) as i64)} ปิด {chart::thai_time(n(&last[\"time\"]) as i64+900)} เวลาไทย · ใช้แท่ง 15 นาทีที่ปิดแล้ว ตรวจตลาดประมาณทุก 1 นาที"}
            }else{p{"รอแท่งราคาเพียงพอสำหรับคำนวณ RSI/MACD"}}
            div{class:"watch-row",span{"คำแนะนำปัจจุบัน"}strong{"{s(&advice[\"action\"])}"}}
            p{"{decision_reason(s(&advice[\"action\"]),s(&advice[\"reason\"]))}"}
            if !data["real"]["advice_model"].is_null()&&!advice["forecast"].is_null(){p{class:"muted small","โมเดลคาด {n(&advice[\"forecast\"])*100.0:+.2}% · ส่วนต่างหลังต้นทุนและความไม่แน่นอน {n(&advice[\"net_edge\"])*100.0:+.2}% · ต้องเกิน {n(&data[\"real\"][\"advice_policy\"][\"min_edge\"])*100.0:.2}% จึงเสนอซื้อ ยังต้องตรวจราคาจริงอีกครั้ง"}}
            p{class:"account-explainer","ดู RSI/MACD ร่วมกับต้นทุน ความไม่แน่นอน และวงเงิน ตัวเลขตัวเดียวไม่ใช่คำสั่งซื้อขาย"}
        }
        section{class:"panel spaced",h2{"เป้าต่อไม้ ไม่เร่งทำยอดรายวัน"}
            p{"ใช้เป้าขายและจุดทบทวนของแต่ละไม้ตามความผันผวนตลาด ดูผลสะสมสุทธิหลังค่าเครือข่าย ไม่บังคับกำไรขั้นต่ำต่อวันหรือจำนวนครั้งที่ต้องเทรด"}
            p{class:"warning","มี SOL ในไม้เทรดค้าง {n(&r[\"open_layers\"]) as u64} ไม้ · ปิดคอมแล้วราคายังเปลี่ยน แต่แอปเตือนหรือขายให้ไม่ได้ ดูต้นทุน เป้าขาย และจุดทบทวนในตารางไม้ด้านล่างก่อนพัก"}
        }
    }
}
fn decision_reason(action: &str, reason: &str) -> String {
    match action {
        "buy" => "โมเดลผ่านเกณฑ์เบื้องต้น ต้องตรวจวงเงินและราคาสลับจริงก่อนเสนอคำสั่ง".into(),
        "sell" => "มีไม้ถึงเป้าขายหรือจุดทบทวน ดูการ์ดและยืนยันเองใน Phantom".into(),
        "review" => "ถึงเกณฑ์ทบทวนความเสี่ยง พักซื้อเพิ่มและพิจารณาไม้ที่ถืออยู่".into(),
        "paused" => "พักการซื้อเพิ่มอยู่".into(),
        "pending" => "มีคำสั่งค้าง รอผลหรือกระทบยอดก่อนทำรายการใหม่".into(),
        "wait" if reason.contains("stale") || reason.contains("rate") => {
            "ข้อมูลราคาหรือเรตบาทยังไม่พร้อม รอรีเฟรช".into()
        }
        "wait" if reason.contains("candidate") => {
            "ยังไม่ได้เลือกสูตร กดเริ่มเฝ้าตลาดเพื่อเตรียมสูตรเริ่มต้น".into()
        }
        "wait" => "ผลคาดการณ์ยังไม่คุ้มต้นทุนและความไม่แน่นอน จึงรอสัญญาณก่อน".into(),
        _ => reason.into(),
    }
}
