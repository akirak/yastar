use anyhow::anyhow;
use std::collections::HashMap;

use chrono::{Duration, NaiveDate};
use itertools::Itertools;
use plotters::{
    prelude::*,
    style::text_anchor::{HPos, Pos, VPos},
};
use serde::Deserialize;

const FONT_FAMILY: &str = "sans-serif";

const CHART_CAPTION_FONT_SIZE: i32 = 14;

const LABEL_FONT_SIZE: i32 = 12;

fn parse_hex_color(hex: &str) -> anyhow::Result<RGBColor> {
    let hex = hex.trim_start_matches('#');

    if hex.len() != 6 {
        return Err(anyhow!("Invalid hex color"));
    }

    let r = u8::from_str_radix(&hex[0..2], 16)?;
    let g = u8::from_str_radix(&hex[2..4], 16)?;
    let b = u8::from_str_radix(&hex[4..6], 16)?;

    Ok(RGBColor(r, g, b))
}

#[derive(Debug, Deserialize)]
struct ColorEntry {
    color: Option<String>,
    // url: String,
}

fn load_github_colors() -> anyhow::Result<HashMap<String, ColorEntry>> {
    let json_str = include_str!("colors.json");
    let data = serde_json::from_str(json_str)?;
    Ok(data)
}

pub fn draw_star_history_by_language(
    vec: Vec<(NaiveDate, String, i64)>,
    path: &str,
) -> anyhow::Result<()> {
    let drawing_area = plotters::prelude::SVGBackend::new(path, (500, 250)).into_drawing_area();
    let root = drawing_area;

    let from_date = vec.first().unwrap().0 - Duration::days(1);
    let to_date = vec.last().unwrap().0 + Duration::days(1);

    let max_value = vec.iter().map(|(_, _, n)| n.clone()).max().unwrap();

    root.fill(&WHITE)?;

    let mut chart = ChartBuilder::on(&root)
        .caption(
            "Number of stargazers by language",
            (FONT_FAMILY, CHART_CAPTION_FONT_SIZE),
        )
        .x_label_area_size(20)
        .y_label_area_size(20)
        .margin(10)
        .margin_right(30)
        .build_cartesian_2d(from_date..to_date, 0..max_value + 50)?;

    chart
        .configure_mesh()
        .disable_x_mesh()
        .x_labels(5)
        .max_light_lines(2)
        .draw()?;

    let languages = vec
        .iter()
        .map(|(_, lang, _)| lang)
        .unique()
        .collect::<Vec<_>>();

    let centered = Pos::new(HPos::Center, VPos::Top);

    let color_map = load_github_colors()?;

    for &language in languages.iter() {
        let color = match color_map
            .get(language)
            .map(|ent| ent.color.clone())
            .flatten()
        {
            Some(hex_color) => parse_hex_color(hex_color.as_str())?,
            // fallback
            None => RED,
        };

        let items = vec
            .iter()
            .filter_map(|(date, lang, value)| {
                if language.eq(lang) {
                    Some((date.clone(), value.clone()))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();

        let (x, y) = items.get((items.len() / 2) + 1).unwrap().clone();
        let label_style = TextStyle::from((FONT_FAMILY, LABEL_FONT_SIZE).into_font())
            .pos(centered)
            .color(&color);
        chart
            .draw_series([EmptyElement::at((x, y))
                + Text::new(language.to_string(), (-10, -15), &label_style)])?;

        chart
            .draw_series(LineSeries::new(items, color.stroke_width(1)))?
            .label(language)
            .legend(move |(x, y)| PathElement::new(vec![(x, y), (x + 15, y)], color));
    }

    chart
        .configure_series_labels()
        .position(SeriesLabelPosition::UpperLeft)
        .background_style(WHITE)
        .draw()?;

    root.present()?;

    Ok(())
}

pub fn draw_total_star_history(vec: Vec<(NaiveDate, i64)>, path: &str) -> anyhow::Result<()> {
    let drawing_area = plotters::prelude::SVGBackend::new(path, (500, 250)).into_drawing_area();
    let root = drawing_area;

    let from_date = vec.first().unwrap().0 - Duration::days(1);
    let to_date = vec.last().unwrap().0 + Duration::days(1);

    let max_value = vec.last().unwrap().1;

    root.fill(&WHITE)?;

    let mut chart = ChartBuilder::on(&root)
        .caption(
            "Total number of stargazers",
            (FONT_FAMILY, CHART_CAPTION_FONT_SIZE),
        )
        .x_label_area_size(20)
        .y_label_area_size(20)
        .margin(10)
        .margin_right(30)
        .build_cartesian_2d(from_date..to_date, 0..max_value + 50)?;

    chart
        .configure_mesh()
        .disable_x_mesh()
        .x_labels(5)
        .max_light_lines(2)
        .draw()?;

    chart.draw_series(LineSeries::new(vec, &BLACK))?;

    root.present()?;

    Ok(())
}
