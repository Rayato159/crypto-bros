use super::*;

#[component]
pub fn BuyQuoteDetails() -> Element {
    let ctx = use_context::<Context>();
    let data = (ctx.state)();
    let q = &data["watch"]["last_buy_quote"];
    if q.is_null() {
        return rsx! {};
    }
    let passed = q["passed"] == true;
    let expected = &q["expected"];
    let minimum = &q["minimum"];
    rsx! {
        section {class:"panel spaced quote-review",
            div {class:"panel-heading",
                h2 {"ผลตรวจราคาซื้อครั้งล่าสุด"}
                span {class:"chip",if passed {"ผ่านตอนตรวจราคา"}else{"ยังไม่เตรียมซื้อ"}}
            }
            p {class:if passed{"quote-verdict"}else{"quote-verdict cost-warning"},
                if passed {"ค่าใช้จ่ายผ่านเกณฑ์ ณ เวลาที่ตรวจ ทุกคำสั่งยังต้องยืนยันใน Phantom"}
                else {"สัญญาณเข้าได้ แต่ส่วนต่างหลังค่าใช้จ่ายยังไม่ผ่านเกณฑ์ ไม่มีการซื้อหรือหักเงินจากการตรวจนี้"}
            }
            p {"ตรวจเมื่อ {local_time(n(&q[\"checked_at\"]))} · ซื้อ {n(&q[\"input_usdc\"])/1e6:.6} USDC"}
            p {class:"muted","ผลนี้เป็นบันทึกราคาครั้งก่อน ไม่ใช่ราคาสด ลิงก์คำสั่งหมดอายุเมื่อ {local_time(n(&q[\"expires_at\"]))} ต้องตรวจราคาใหม่เมื่อหมดอายุหรือสัญญาณเปลี่ยน"}
            div {class:"table-wrap",table {class:"quote-table",
                thead {tr {th {"หากขายส่วนเทรดที่ราคาเป้า"}th {"รับ SOL ตามที่คาด"}th {"รับ SOL ขั้นต่ำตาม slippage"}}}
                tbody {
                    tr {td {"SOL ที่รับทั้งหมด"}td {"{n(&expected[\"received_sol\"])/1e9:.9}"}td {"{n(&minimum[\"received_sol\"])/1e9:.9}"}}
                    tr {td {"SOL ส่วนเทรด 80%"}td {"{n(&expected[\"tradable_sol\"])/1e9:.9}"}td {"{n(&minimum[\"tradable_sol\"])/1e9:.9}"}}
                    tr {td {"ราคาต่อ SOL จากใบเสนอราคา · USDC/SOL"}td {"{n(&expected[\"entry_price\"])/1e6:.6}"}td {"{n(&minimum[\"entry_price\"])/1e6:.6}"}}
                    tr {td {"เป้าจากต้นทุนซื้อ + ระยะ ATR เดิม · USDC/SOL"}td {"{n(&expected[\"target_price\"])/1e6:.6}"}td {"{n(&minimum[\"target_price\"])/1e6:.6}"}}
                    tr {td {"ต้นทุนเฉพาะส่วนที่ขาย · USDC"}td {"{n(&expected[\"cost_usdc\"])/1e6:.6}"}td {"{n(&minimum[\"cost_usdc\"])/1e6:.6}"}}
                    tr {td {"ค่าเครือข่ายตอนซื้อ เฉพาะส่วนที่ขาย · USDC"}td {"{n(&expected[\"entry_fee_usdc\"])/1e6:.6}"}td {"{n(&minimum[\"entry_fee_usdc\"])/1e6:.6}"}}
                    tr {td {"ค่าขายและ slippage ประมาณ 0.30% · USDC"}td {"{n(&expected[\"exit_swap_cost_usdc\"])/1e6:.6}"}td {"{n(&minimum[\"exit_swap_cost_usdc\"])/1e6:.6}"}}
                    tr {td {"ค่าเครือข่ายตอนขายโดยประมาณ · USDC"}td {"{n(&expected[\"exit_network_cost_usdc\"])/1e6:.6}"}td {"{n(&minimum[\"exit_network_cost_usdc\"])/1e6:.6}"}}
                    tr {class:"quote-total",td {"กำไร/ขาดทุนสุทธิโดยประมาณ · USDC"}td {"{n(&expected[\"net_usdc\"])/1e6:+.6}"}td {"{n(&minimum[\"net_usdc\"])/1e6:+.6}"}}
                    tr {td {"ต้องเหลือมากกว่า 0.20% ของต้นทุนส่วนที่ขาย · USDC"}td {"{n(&expected[\"required_net_usdc\"])/1e6:.6}"}td {"{n(&minimum[\"required_net_usdc\"])/1e6:.6}"}}
                }
            }}
            p {class:"muted","เป้าคำนวณจากต้นทุนที่ซื้อได้ด้วยสูตรเดียวกับตอนบันทึกไม้ ไม่เพิ่มระยะ ATR เพื่อบังคับให้ผ่าน ส่วนสำรอง SOL 20% ยังถืออยู่และไม่นับเป็นกำไรที่ขายได้"}
            p {class:"muted","ค่าขายยังเป็นประมาณการ ไม่ใช่ราคาขายที่ล็อกไว้ ค่าเครือข่ายขาออกใช้จำนวน SOL เท่ากับค่าขาเข้าโดยประมาณ ราคาจริงและค่าธรรมเนียมตอนขายอาจต่างไป เป้าไม่ใช่คำทำนายว่าจะไปถึง"}
            if n(&q["rent_delta_lamports"])!=0.0 {
                p {class:"muted","เงินประกันบัญชีที่เปลี่ยนแปลง {n(&q[\"rent_delta_lamports\"])/1e9:+.9} SOL แยกจากค่าเครือข่ายและกำไร เพราะเป็นเงินประกันที่อาจเรียกคืนได้"}
            }
            if !passed {p {"รอตรวจราคาใหม่จากการ์ดด้านบนได้ แอปจะตรวจทุกด่านอีกครั้งและไม่เพิ่มวงเงินซื้อเอง"}}
        }
    }
}
