use super::*;

pub fn thai_time(time: i64) -> String {
    time.checked_add(7 * 3600)
        .and_then(|t| chrono::DateTime::from_timestamp(t, 0))
        .map(|t| t.format("%d/%m/%Y %H:%M").to_string())
        .unwrap_or_else(|| "—".into())
}
#[component]
pub fn CandleChart(candles: Value) -> Element {
    let candles: Vec<_> = items(&candles).into_iter().rev().take(80).rev().collect();
    let mut selected = use_signal(|| None::<usize>);
    if candles.is_empty() {
        return rsx! {div{class:"chart-empty","ยังไม่มีแท่งราคา กดรีเฟรชตลาดก่อน"}};
    }
    let low = candles
        .iter()
        .map(|c| n(&c["low"]))
        .fold(f64::INFINITY, f64::min);
    let high = candles
        .iter()
        .map(|c| n(&c["high"]))
        .fold(f64::NEG_INFINITY, f64::max);
    let range = (high - low).max(0.01);
    let y = |p: f64| 270.0 - (p - low) / range * 230.0;
    let step = 850.0 / candles.len() as f64;
    let active = selected().and_then(|i| candles.get(i).map(|c| (i, c)));
    rsx! {
        div{class:"candle-chart",onmouseleave:move |_|selected.set(None),
            svg{view_box:"0 0 960 310",role:"group","aria-label":"กราฟแท่ง SOL USDC เลื่อนเมาส์หรือกด Tab เพื่อดูรายละเอียด",
                for i in 0..5 {line{x1:"10",y1:"{40.0+i as f64*57.5}",x2:"875",y2:"{40.0+i as f64*57.5}",stroke:"#2a2f25",stroke_dasharray:"3 5"}
                    text{x:"892",y:"{44.0+i as f64*57.5}",fill:"#89937e",font_size:"12","{high-i as f64*range/4.0:.2}"}}
                for (i,c) in candles.iter().enumerate(){
                    g{
                        line{x1:"{14.0+i as f64*step}",x2:"{14.0+i as f64*step}",y1:"{y(n(&c[\"high\"]))}",y2:"{y(n(&c[\"low\"]))}",stroke:if n(&c["close"])>=n(&c["open"]){"#b8e787"}else{"#c77c66"}}
                        rect{x:"{14.0+i as f64*step-step*0.26}",y:"{y(n(&c[\"open\"]).max(n(&c[\"close\"])))}",width:"{step*0.52}",height:"{(y(n(&c[\"open\"]).min(n(&c[\"close\"])))-y(n(&c[\"open\"]).max(n(&c[\"close\"])))).max(1.5)}",rx:"1",fill:if n(&c["close"])>=n(&c["open"]){"#b8e787"}else{"#c77c66"}}
                    }
                }
                for (i,c) in candles.iter().enumerate(){rect{class:"candle-hit",x:"{14.0+i as f64*step-step*0.5}",y:"20",width:"{step}",height:"260",fill:"transparent",tabindex:"0",role:"img",
                    "aria-label":"แท่ง {thai_time(n(&c[\"time\"]) as i64)} ราคาปิด {n(&c[\"close\"]):.4} USDC",
                    onmouseenter:move |_|selected.set(Some(i)),onfocus:move |_|selected.set(Some(i)),onclick:move |_|selected.set(Some(i)),onblur:move |_|selected.set(None)}}
                text{x:"14",y:"302",fill:"#89937e",font_size:"12","แท่ง 15 นาทีที่ปิดแล้ว · เวลาไทย UTC+7"}
            }
            if let Some((i,c))=active {
                div{class:if i>candles.len()/2{"candle-tooltip left"}else{"candle-tooltip"},role:"tooltip",
                    strong{"{thai_time(n(&c[\"time\"]) as i64)}"}
                    p{class:"muted small","เวลาเปิดแท่ง · ไทย UTC+7 · SOL/USDC"}
                    dl{
                        dt{"เปิด"}dd{"{n(&c[\"open\"]):.4} USDC"}
                        dt{"สูงสุด"}dd{"{n(&c[\"high\"]):.4} USDC"}
                        dt{"ต่ำสุด"}dd{"{n(&c[\"low\"]):.4} USDC"}
                        dt{"ปิด"}dd{"{n(&c[\"close\"]):.4} USDC"}
                        dt{"เปลี่ยนแปลง"}dd{if n(&c["open"])>0.0{"{(n(&c[\"close\"])/n(&c[\"open\"])-1.0)*100.0:+.2}%"}else{"—"}}
                        dt{"ปริมาณ"}dd{"{n(&c[\"volume\"]):.4} SOL"}
                    }
                }
            }
        }
    }
}
