"""Cut Noto Sans down to the glyphs the countdown actually draws.

The full CJK files stay out of the binary. Regenerating needs the woff
sources in tools/font-src (fontsource Noto Sans SC/JP/KR, weight 400).
"""

from pathlib import Path

from fontTools.subset import Options, Subsetter
from fontTools.ttLib import TTFont

ROOT = Path(__file__).resolve().parents[1]
SRC = ROOT / "tools" / "font-src"
OUT = ROOT / "src" / "fonts"

ASCII = "".join(chr(c) for c in range(0x20, 0x7F))

SC = ASCII + (
    "距离下班已今天辛苦了再见明时间设置上次日背景默认大海极光山影语言中文倒计"
    "下班提醒颜色竹绿樱粉天蓝鎏金"
    "贴边收起开启关闭文字显示始终鼠标移入"
    "Until off work Nice today See you Tomorrow Settings Start End Backdrop "
    "Plain Sea Aurora Ridge Language Off-work countdown EN"
)
JP = ASCII + "退勤まで済み今日もお疲れ様。また明日設定出勤翌日背景標準海オーロラ山影言語日本語カウント退勤色竹緑桜空色金粉収納するしない文字常時重ね"
KR = ASCII + "퇴근까지함오늘도 수고했어요.내일 봐요시간설정출근다음날배경기본바다오로라산그림자언어한국어카운트다운퇴근색초록분홍하늘금빛가장자리숨기기켜기끄기글자항상마우스"


def rewrite_name(font: TTFont) -> None:
    table = font["name"]
    for platform, encoding, language in ((3, 1, 0x409), (1, 0, 0)):
        table.setName("OffWork Sans", 1, platform, encoding, language)
        table.setName("Regular", 2, platform, encoding, language)
        table.setName("OffWork Sans", 4, platform, encoding, language)
        table.setName("OffWorkSans", 6, platform, encoding, language)
        table.setName("OffWork Sans", 16, platform, encoding, language)
        table.setName("Regular", 17, platform, encoding, language)


def subset(src: Path, text: str, dest: Path) -> None:
    font = TTFont(src)
    options = Options()
    options.layout_features = ["*"]
    options.name_IDs = ["*"]
    options.notdef_outline = True
    options.drop_tables = ["vhea", "vmtx", "STAT", "BASE"]
    cutter = Subsetter(options=options)
    cutter.populate(text=text)
    cutter.subset(font)
    rewrite_name(font)
    dest.parent.mkdir(parents=True, exist_ok=True)
    font.save(dest)
    print(f"{dest.name} {dest.stat().st_size} bytes, {len(font.getGlyphOrder())} glyphs")


def main() -> None:
    subset(SRC / "sc.woff", SC, OUT / "sc.ttf")
    subset(SRC / "jp.woff", JP, OUT / "jp.ttf")
    subset(SRC / "kr.woff", KR, OUT / "kr.ttf")


if __name__ == "__main__":
    main()
