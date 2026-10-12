// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Render reviewed suite results without replacing the existing macOS report.
use anyhow::{Context, Result};
use serde_json::Value;
use std::path::Path;

const START: &str = "<!-- ibis-bench-windows:start -->";
const END: &str = "<!-- ibis-bench-windows:end -->";
const COLUMNS: [&str; 4] = ["ibisoffice", "msoffice", "libreoffice", "onlyoffice"];
const HEADER: &str = "| ファイル | ibisOffice | Microsoft 365 | LibreOffice | ONLYOFFICE |\n|---|---:|---:|---:|---:|\n";

fn office(kind: &str) -> &str {
    match kind {
        "pptx" => "msoffice-powerpoint",
        "docx" => "msoffice-word",
        _ => "msoffice-excel",
    }
}

fn load(runs: &Path, tag: &str) -> Result<Option<Value>> {
    let path = runs.join(format!("{tag}.json"));
    if !path.is_file() {
        return Ok(None);
    }
    Ok(Some(
        serde_json::from_slice(&std::fs::read(&path)?)
            .with_context(|| format!("invalid result {}", path.display()))?,
    ))
}

fn eligible(run: &Value, pdf: bool) -> bool {
    run["formal"].as_bool() == Some(true)
        && run["suite_exit_ok"].as_bool() == Some(true)
        && run["reviewed"].as_bool() == Some(true)
        && (pdf || run["build_running_after"].as_bool() == Some(false))
}

fn value(
    runs: &Path,
    tag: &str,
    pdf: bool,
) -> Result<Option<(Option<f64>, f64, bool, Option<u64>)>> {
    let Some(run) = load(runs, tag)? else {
        return Ok(None);
    };
    if !eligible(&run, pdf) {
        return Ok(None);
    }
    if pdf {
        return Ok(Some((
            run["seconds"].as_f64(),
            600.0,
            run["ok"].as_bool() != Some(true),
            run["pages"].as_u64(),
        )));
    }
    let analysis: Value =
        serde_json::from_slice(&std::fs::read(runs.join(format!("{tag}.analysis.json")))?)?;
    Ok(Some((
        analysis["end"].as_f64(),
        run["cap"].as_f64().context("missing cap")?,
        run["capped"].as_bool() != Some(false) || run["shown"].as_bool() == Some(false),
        None,
    )))
}

fn cell(result: Option<(Option<f64>, f64, bool, Option<u64>)>, base: Option<f64>) -> String {
    let Some((seconds, cap, capped, pages)) = result else {
        return "計測・確認待ち".into();
    };
    if capped {
        return format!("N/A（上限 {cap:.0}s）");
    }
    let Some(seconds) = seconds.filter(|s| s.is_finite() && *s >= 0.0) else {
        return "計測・確認待ち".into();
    };
    let mut text = format!("{seconds:.2}s");
    if let Some(base) = base.filter(|b| *b > 0.0) {
        text += &format!("（{:.1} 倍）", seconds / base);
    }
    if let Some(pages) = pages {
        text += &format!("<br>{pages} ページ");
    }
    text
}

fn table(runs: &Path, scenario: &str) -> Result<String> {
    let mut out = HEADER.to_owned();
    for (kind, file) in crate::pdf::DOCS {
        let base_result = value(
            runs,
            &format!("{scenario}-ibisoffice-{kind}"),
            scenario == "pdf",
        )?;
        let base = base_result.filter(|v| !v.2).and_then(|v| v.0);
        out += &format!("| {file} |");
        for column in COLUMNS {
            let app = if column == "msoffice" {
                office(kind)
            } else {
                column
            };
            out += &format!(
                " {} |",
                cell(
                    value(runs, &format!("{scenario}-{app}-{kind}"), scenario == "pdf")?,
                    if column == "ibisoffice" { None } else { base }
                )
            );
        }
        out += "\n";
    }
    Ok(out)
}

fn replace_section(old: &str, section: &str) -> Result<String> {
    match (old.find(START), old.find(END)) {
        (None, None) => Ok(format!("{}\n\n{section}\n", old.trim_end())),
        (Some(start), Some(end))
            if start < end && old.matches(START).count() == 1 && old.matches(END).count() == 1 =>
        {
            Ok(format!(
                "{}{section}{}",
                &old[..start],
                &old[end + END.len()..]
            ))
        }
        _ => anyhow::bail!("invalid Windows section markers; existing report left untouched"),
    }
}

pub fn run(root: &Path) -> Result<()> {
    let runs = root.join("tasks/benchmark/runs-windows");
    let mut section = format!(
        "{START}\n\n## Windows の計測\n\n正式な一括計測に成功し、画像・PDFを確認した結果だけを載せる。検証用の値や未確認の結果は「計測・確認待ち」とする。各1回。括弧内は ibisOffice の時間に対する比率。\n\n"
    );
    let environment = runs.join("environment.json");
    if environment.is_file() {
        let data: Value = serde_json::from_slice(&std::fs::read(environment)?)?;
        section += &format!(
            "計測環境は `runs-windows/environment.json` に記録。採取時刻（Unix秒）：{}。\n\n",
            data["recorded_at_unix"]
        );
        section += &format!(
            "環境：{}（{}）、{}、メモリ {:.1} GiB。画面 {}×{}、システムDPI {}（倍率 {:.0}%）、文書ウィンドウ1440×900物理ピクセル。\n\n",
            data["os"].as_str().unwrap_or("未記録"),
            data["os_version"].as_str().unwrap_or("未記録"),
            data["cpu"]
                .as_array()
                .map(|v| v
                    .iter()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>()
                    .join(", "))
                .unwrap_or_else(|| "未記録".into()),
            data["memory_bytes"].as_f64().unwrap_or(0.0) / (1024.0 * 1024.0 * 1024.0),
            data["display_pixels"][0],
            data["display_pixels"][1],
            data["system_dpi"],
            data["system_dpi"].as_f64().unwrap_or(96.0) / 96.0 * 100.0
        );
        section += &format!(
            "製品：ibisOffice {}、Microsoft 365 {}、LibreOffice {}、ONLYOFFICE {}。\n\n",
            env!("CARGO_PKG_VERSION"),
            data["versions"]["WINWORD.EXE"].as_str().unwrap_or("未記録"),
            data["versions"]["soffice.exe"].as_str().unwrap_or("未記録"),
            data["versions"]["DesktopEditors.exe"]
                .as_str()
                .unwrap_or("未記録")
        );
    } else {
        section += "計測環境の確定・記録待ち。\n\n";
    }
    section += "Windows Graphics Capture と QPC を使用。192×120格子、輝度差24超、変化率0.2%超で最後の描画変化を求める。動画を作らず、時刻・格子と終了前後のPNGを保存する。文書の窓は物理1440×900ピクセル。OSの表示倍率は変更しない。\n\nファイルを開く計測は、起動済みアプリの開くダイアログにパスを設定して読み返し、Openを送る直前から計る。Macの `open -a` とは開始操作が異なる。CPUは対象ウィンドウのプロセスのみで、ヘルパーを含まない。XLSXは中央の色付き画素が0.5%を超えるまで待つ。PDFはプロセス起動から終了までを計り、Windows.Data.Pdfでページ数を数える。\n\n";
    section += "起動はプロセス起動前から画面全体を収録し、最初に所有ウィンドウが見つかった時点で位置20,20・1440×900に固定する。この固定操作の時間も含む。起動画像は最近使用した個人ファイル等が写るため、切り出し画像もローカルのtarget内に保持し公開しない。\n\n";
    section += "PowerPointの公開pptxは埋め込みフォント（ヒラギノ角ゴ Pro W3）の読み込み失敗通知が出る。同一プロセス・通知タイトル・失敗文言・唯一のOKボタンを確認して自動で閉じ、その待ち時間を含める。フォントのインストール操作は行わない。資料コピーのパスとスライド数をCOMで確認してから末尾へ移動する。\n\n";
    section += "ONLYOFFICEのpptxはUIAのスライド数表示を待って読み込み中の静止画を除外する。LibreOfficeのpptx末尾は最後のスライドのキャンバスを確認した後にEnd入力を止める。pptx末尾は左の縮小画像一覧も含め、20秒の静止を確認する。UIA確認と移動操作の時間は計測に含む。\n\n";
    section += "XLSX末尾はOverviewの最後のグラフを対象に、ExcelはR213、ibisOffice・LibreOfficeはA193:R213、ONLYOFFICEはA203へ移動する。各製品の表示倍率を保つため、右端や下端が画面外に続く場合がある。ibisOfficeのdocx末尾は149/149ページの本文を確認した（ページの下は画面の外に続く）。ibisOfficeのpptxは、開いた直後からキーボードのフォーカスが左のサムネイル一覧にあるので、クリックせずにEndで最後のスライドへ移る。LibreOffice・ONLYOFFICEのdocx初回表示では総ページ数の確定が本文表示より後に続く。末尾到達や先頭表示の確認は、全ページの描画完了・品質を保証しない。\n\n";
    section += "4製品とも2026-10-12に、表示倍率100%で計った（ibisOfficeのPDF変換は、その後にlayout_lineの結果の再利用を入れて計り直した）。2026-10-02に表示倍率125%で計った値は、条件が違うので載せていない。起動は、アプリを初めて起動する回（コールドスタート）が遅いので、2回目の値を載せた（1回目の値は下の注記）。各回の正式結果・終了ログ・確認画像はtarget内に保持する。\n\n";
    section += "### Windows 表1：起動\n\n";
    section += "| | ibisOffice | Microsoft 365 | LibreOffice | ONLYOFFICE |\n|---|---:|---:|---:|---:|\n| ファイルなしで起動 |";
    let base = value(&runs, "startup-ibisoffice", false)?
        .filter(|v| !v.2)
        .and_then(|v| v.0);
    for column in COLUMNS {
        let text = if column == "msoffice" {
            let mut parts = Vec::new();
            for (app, name) in [
                ("msoffice-powerpoint", "PowerPoint"),
                ("msoffice-word", "Word"),
                ("msoffice-excel", "Excel"),
            ] {
                parts.push(format!(
                    "{name} {}",
                    cell(value(&runs, &format!("startup-{app}"), false)?, base)
                ));
            }
            parts.join("<br>")
        } else {
            cell(
                value(&runs, &format!("startup-{column}"), false)?,
                if column == "ibisoffice" { None } else { base },
            )
        };
        section += &format!(" {text} |");
    }
    for (scenario, heading) in [
        ("firstview", "表2：ファーストビュー"),
        ("end", "表3：末尾ページ"),
        ("pdf", "表4：PDF変換"),
    ] {
        section += &format!("\n\n### Windows {heading}\n\n{}", table(&runs, scenario)?);
    }
    section += "\nPDF変換：ibisOffice `--export pdf`、Microsoft 365はRustからCOM、LibreOfficeは専用プロフィールでheadless変換、ONLYOFFICEは同梱x2t。入力は公開データのコピーを使用。格子・PNG・PDFは `target/benchmark-windows/` にローカル保存。\n\n";
    section += "ページ数やページ区切りは製品によって異なる。先頭・末尾の確認は変換ができたことを確かめる検査で、全ページの描画品質の比較ではない。XLSXの末尾には、空白や文字の一部だけのページが出力される場合がある。個別の確認内容は結果JSONの `review_note` に記録する。\n\n";
    section += END;
    let path = root.join("tasks/benchmark/benchmark.md");
    let old = std::fs::read_to_string(&path)?;
    std::fs::write(&path, replace_section(&old, &section)?)?;
    println!("updated {}", path.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preserves_handwritten_mac_notes_and_rejects_broken_markers() {
        let old = format!("Mac annotation\n{START}\nold\n{END}\nmanual footer");
        let section = format!("{START}\nnew\n{END}");
        assert_eq!(
            replace_section(&old, &section).unwrap(),
            format!("Mac annotation\n{section}\nmanual footer")
        );
        assert!(replace_section(&format!("Mac\n{START}"), &section).is_err());
    }
    #[test]
    fn validation_or_failed_cleanup_never_enters_comparison_table() {
        let mut result = serde_json::json!({"formal":true,"reviewed":true,"suite_exit_ok":true,"build_running_after":false});
        assert!(eligible(&result, false));
        result["suite_exit_ok"] = false.into();
        assert!(!eligible(&result, false));
        assert!(!eligible(
            &serde_json::json!({"status":"validation","seconds":1.0}),
            true
        ));
    }
}
