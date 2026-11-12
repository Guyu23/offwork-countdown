//! The four languages the card can speak. Segment names stay in their own
//! script so a reader can always find the one they want.

pub const LANG_COUNT: u8 = 4;

#[derive(Clone, Copy)]
pub struct Phrases {
    pub away: &'static str,
    pub done: &'static str,
    pub farewell: &'static str,
    pub bye: &'static str,
    pub settings: &'static str,
    pub start: &'static str,
    pub end: &'static str,
    pub next_day: &'static str,
    pub backdrop: &'static str,
    pub themes: [&'static str; 4],
    pub language: &'static str,
    pub accent: &'static str,
    pub accents: [&'static str; 4],
    pub tuck: &'static str,
    pub tucks: [&'static str; 2],
    pub caption: &'static str,
    pub captions: [&'static str; 2],
    pub title: &'static str,
}

const ZH: Phrases = Phrases {
    away: "距离下班",
    done: "已下班",
    farewell: "今天辛苦了。",
    bye: "明天见",
    settings: "设置",
    start: "上班",
    end: "下班",
    next_day: "次日",
    backdrop: "背景",
    themes: ["默认", "大海", "极光", "山影"],
    language: "语言",
    accent: "下班提醒颜色",
    accents: ["竹绿", "樱粉", "天蓝", "鎏金"],
    tuck: "贴边收起",
    tucks: ["开启", "关闭"],
    caption: "文字显示",
    captions: ["始终显示", "鼠标移入"],
    title: "下班倒计时",
};

const JA: Phrases = Phrases {
    away: "退勤まで",
    done: "退勤済",
    farewell: "今日もお疲れ様。",
    bye: "また明日",
    settings: "設定",
    start: "出勤",
    end: "退勤",
    next_day: "翌日",
    backdrop: "背景",
    themes: ["標準", "海", "オーロラ", "山影"],
    language: "言語",
    accent: "退勤色",
    accents: ["竹緑", "桜", "空色", "金粉"],
    tuck: "収納",
    tucks: ["する", "しない"],
    caption: "文字",
    captions: ["常時", "重ね"],
    title: "退勤カウント",
};

const EN: Phrases = Phrases {
    away: "Until off",
    done: "Off work",
    farewell: "Nice work today.",
    bye: "See you",
    settings: "Settings",
    start: "Start",
    end: "End",
    next_day: "+1",
    backdrop: "Backdrop",
    themes: ["Plain", "Sea", "Aurora", "Ridge"],
    language: "Language",
    accent: "Reminder",
    accents: ["Bamboo", "Sakura", "Sky", "Gilt"],
    tuck: "Edge tuck",
    tucks: ["On", "Off"],
    caption: "Text",
    captions: ["Always", "Hover"],
    title: "Off-work countdown",
};

const KO: Phrases = Phrases {
    away: "퇴근까지",
    done: "퇴근함",
    farewell: "오늘도 수고했어요.",
    bye: "내일 봐요",
    settings: "설정",
    start: "출근",
    end: "퇴근",
    next_day: "다음날",
    backdrop: "배경",
    themes: ["기본", "바다", "오로라", "산그림자"],
    language: "언어",
    accent: "퇴근색",
    accents: ["초록", "분홍", "하늘", "금빛"],
    tuck: "가장자리",
    tucks: ["켜기", "끄기"],
    caption: "글자",
    captions: ["항상", "마우스"],
    title: "퇴근 카운트다운",
};

/// Native names for the language switch. Not translated with the UI.
pub const LANG_NAMES: [&str; 4] = ["中文", "日本語", "EN", "한국어"];

pub fn phrases(lang: u8) -> Phrases {
    match lang {
        1 => JA,
        2 => EN,
        3 => KO,
        _ => ZH,
    }
}

pub fn footer_off(lang: u8, time: &str) -> String {
    match lang {
        1 => format!("退勤 {time}"),
        2 => format!("Off {time}"),
        3 => format!("퇴근 {time}"),
        _ => format!("下班时间 {time}"),
    }
}

pub fn footer_next(lang: u8, time: &str) -> String {
    match lang {
        1 => format!("明日 {time}"),
        2 => format!("Tomorrow {time}"),
        3 => format!("내일 {time}"),
        _ => format!("明天 {time}"),
    }
}

pub fn lang_token(lang: u8) -> &'static str {
    match lang {
        1 => "ja",
        2 => "en",
        3 => "ko",
        _ => "zh",
    }
}

pub fn lang_from_token(token: &str) -> u8 {
    match token {
        "ja" => 1,
        "en" => 2,
        "ko" => 3,
        _ => 0,
    }
}

pub fn accent_token(accent: u8) -> &'static str {
    match accent {
        1 => "pink",
        2 => "sky",
        3 => "gold",
        _ => "green",
    }
}

pub fn tuck_enabled(token: &str) -> bool {
    token != "stay"
}

pub fn tuck_token(enabled: bool) -> &'static str {
    if enabled { "tuck" } else { "stay" }
}

pub fn caption_mode(token: &str) -> u8 {
    if token == "hover" { 1 } else { 0 }
}

pub fn caption_token(mode: u8) -> &'static str {
    if mode == 1 { "hover" } else { "show" }
}

pub fn accent_from_token(token: &str) -> u8 {
    match token {
        "pink" => 1,
        "sky" => 2,
        "gold" => 3,
        _ => 0,
    }
}
