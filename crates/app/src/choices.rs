//! Agent-to-user questions: a popup of options the user picks from. The pick
//! is mailed back to the asking agent; "choose later" just hides the popup.

use std::path::PathBuf;

use gpui::{AnyElement, Context, FontWeight, ObjectFit, StyledImage, div, img, prelude::*, px};
use serde_json::Value;
use termden_core::{Choice, ChoiceOption, HarnessId};

use crate::state::TermdenApp;
use crate::widgets::{ICON_FONT, icons};

pub fn parse(from: HarnessId, body: &Value) -> Result<Choice, String> {
    {
        let text = |v: &Value, max: usize| {
            v.as_str()
                .unwrap_or("")
                .trim()
                .chars()
                .take(max)
                .collect::<String>()
        };
        let title = text(&body["title"], 80);
        if title.is_empty() {
            return Err("missing title".into());
        }
        let options: Vec<_> = body["options"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|o| ChoiceOption {
                label: text(&o["label"], 160),
                description: text(&o["description"], 240),
                image: o["image"]
                    .as_str()
                    .map(PathBuf::from)
                    .filter(|p| p.is_absolute() && p.is_file()),
            })
            .collect();
        if !(2..=8).contains(&options.len()) || options.iter().any(|o| o.label.is_empty()) {
            return Err("need 2-8 options, each with a label".into());
        }
        Ok(Choice {
            from,
            title,
            question: text(&body["question"], 300),
            options,
        })
    }
}

impl TermdenApp {
    pub fn add_choice(&mut self, choice: Choice) {
        let name = self
            .harness(choice.from)
            .map_or("agent".into(), |(_, h)| h.name.clone());
        self.log(name, format!("needs your choice: {}", choice.title), 1);
        self.choices.push(choice);
        self.choice_open = true;
        self.save();
    }

    /// Mail the pick to the asking agent and move on to the next question.
    pub fn answer_choice(&mut self, index: usize) {
        if self.choices.is_empty() || index >= self.choices[0].options.len() {
            return;
        }
        let choice = self.choices.remove(0);
        let option = &choice.options[index];
        let text = format!(
            "I chose option {} \"{}\" for \"{}\".",
            index + 1,
            option.label,
            choice.title
        );
        self.send_mail(None, choice.from, text);
        self.choice_open = !self.choices.is_empty();
        self.save();
    }

    /// Drop the front question without picking; the asking agent is told.
    pub fn skip_choice(&mut self) {
        if self.choices.is_empty() {
            return;
        }
        let choice = self.choices.remove(0);
        let text = format!(
            "I skipped your question \"{}\" without choosing; decide yourself or ask again later.",
            choice.title
        );
        self.send_mail(None, choice.from, text);
        self.choice_open = !self.choices.is_empty();
        self.save();
    }
}

/// "N questions waiting" strip shown while the popup is dismissed.
pub fn banner(app: &TermdenApp, cx: &mut Context<TermdenApp>) -> Option<AnyElement> {
    if app.choice_open || app.choices.is_empty() {
        return None;
    }
    let theme = &app.theme;
    let asker = app
        .harness(app.choices[0].from)
        .map_or("An agent".into(), |(_, h)| h.name.clone());
    let more = app.choices.len() - 1;
    Some(
        div()
            .id("choice-banner")
            .mx(px(9.))
            .mb(px(4.))
            .h(px(32.))
            .px(px(10.))
            .flex()
            .items_center()
            .gap(px(8.))
            .rounded(px(8.))
            .bg(theme.blue.opacity(0.14))
            .border_1()
            .border_color(theme.blue.opacity(0.3))
            .cursor_pointer()
            .hover(|s| s.opacity(0.85))
            .on_click(cx.listener(|this, _, _, cx| {
                this.choice_open = true;
                this.save();
                cx.notify();
            }))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .truncate()
                    .text_size(theme.ui(10.5))
                    .text_color(theme.blue)
                    .child(format!(
                        "{asker} is waiting: {}{}",
                        app.choices[0].title,
                        if more > 0 {
                            format!(" (+{more})")
                        } else {
                            String::new()
                        }
                    )),
            )
            .child(
                div()
                    .text_size(theme.ui(10.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme.blue)
                    .child("Choose"),
            )
            .child(
                crate::widgets::icon_button("choice-skip", icons::CLOSE, theme)
                    .size(px(20.))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.skip_choice();
                        cx.stop_propagation();
                        cx.notify();
                    })),
            )
            .into_any_element(),
    )
}

pub fn render(app: &TermdenApp, cx: &mut Context<TermdenApp>) -> Option<AnyElement> {
    if !app.choice_open && !app.popups[crate::popup::CHOICE].closing() {
        return None;
    }
    let choice = app.choices.first()?;
    let theme = &app.theme;
    let asker = app
        .harness(choice.from)
        .map_or("An agent".into(), |(_, h)| h.name.clone());
    let accent = theme.accent(1);

    // Sized to the options and the window, never bigger than the window.
    let (vw, vh) = app.viewport;
    let size = (
        (choice.options.len().min(4) as f32 * 230.0 + 60.0)
            .clamp(480.0, 1000.0)
            .min(vw - 48.0),
        (vh - 48.0).min(820.0),
    );
    // Text-only options (or a narrow window) read better as full-width rows.
    let narrow = size.0 - 36.0 < choice.options.len().min(4) as f32 * 200.0;
    let rows = narrow || choice.options.iter().all(|o| o.image.is_none());

    let cols = choice.options.len().min(4) as f32;
    let card_w = ((size.0 - 40.0 - 12.0 * (cols - 1.0)) / cols).floor();

    let cards = choice
        .options
        .iter()
        .enumerate()
        .map(|(i, option)| {
            let badge = div()
                .flex_shrink_0()
                .size(px(18.))
                .rounded(px(5.))
                .bg(accent.opacity(0.18))
                .text_color(accent)
                .flex()
                .items_center()
                .justify_center()
                .text_size(theme.ui(9.5))
                .font_weight(FontWeight::SEMIBOLD)
                .child(format!("{}", i + 1));
            let preview = (!rows || option.image.is_some()).then(|| {
                let mut frame = div();
                if rows {
                    frame = frame.w(px(72.)).h(px(54.)).flex_shrink_0();
                } else {
                    frame.style().aspect_ratio = Some(4. / 3.);
                    frame = frame.w_full().max_h(px(260.));
                }
                frame
                    .rounded(px(8.))
                    .bg(theme.crust)
                    .flex()
                    .items_center()
                    .justify_center()
                    .overflow_hidden()
                    .map(|d| match &option.image {
                        Some(path) => d.child(
                            img(path.clone())
                                .w_full()
                                .h_full()
                                .object_fit(ObjectFit::Contain),
                        ),
                        None => d
                            .text_size(theme.ui(28.))
                            .text_color(theme.overlay.opacity(0.5))
                            .child(format!("{}", i + 1)),
                    })
            });
            let label = div()
                .min_w(px(0.))
                .text_size(theme.ui(11.5))
                .font_weight(FontWeight::MEDIUM)
                .text_color(theme.text)
                .child(option.label.clone());
            let description = (!option.description.is_empty()).then(|| {
                div()
                    .min_w(px(0.))
                    .text_size(theme.ui(10.))
                    .text_color(theme.subtext)
                    .child(option.description.clone())
            });
            let base = div()
                .id(("choice-option", i))
                .flex()
                .gap(px(10.))
                .p(px(10.))
                .rounded(px(10.))
                .bg(theme.surface0.opacity(0.4))
                .border_1()
                .border_color(theme.text.opacity(0.08))
                .cursor_pointer()
                .hover(move |s| s.border_color(accent).bg(accent.opacity(0.10)))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.answer_choice(i);
                    cx.notify();
                }));
            if rows {
                base.w_full()
                    .items_center()
                    .child(badge)
                    .children(preview)
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .flex()
                            .flex_col()
                            .gap(px(2.))
                            .child(label)
                            .children(description),
                    )
            } else {
                base.w(px(card_w))
                    .flex_shrink_0()
                    .flex_col()
                    .children(preview)
                    .child(
                        div()
                            .flex()
                            .gap(px(6.))
                            .child(badge)
                            .child(div().flex_1().min_w(px(0.)).child(label)),
                    )
                    .children(description)
            }
        })
        .collect::<Vec<_>>();

    let card = div()
        .id("choice-card")
        .w(px(size.0))
        .max_h(px(size.1))
        .flex()
        .flex_col()
        .rounded(px(14.))
        .bg(theme.mantle)
        .border_1()
        .border_color(theme.text.opacity(0.10))
        .shadow_lg()
        .overflow_hidden()
        .child(crate::popup::drag_handle(
            div()
                .flex()
                .items_center()
                .gap(px(10.))
                .px(px(18.))
                .py(px(12.))
                .flex_shrink_0()
                .border_b_1()
                .border_color(theme.text.opacity(0.06))
                .child(
                    div()
                        .flex_1()
                        .flex()
                        .flex_col()
                        .child(
                            div()
                                .text_size(theme.ui(15.))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(theme.text)
                                .child(choice.title.clone()),
                        )
                        .child(
                            div()
                                .text_size(theme.ui(10.))
                                .text_color(theme.overlay)
                                .child(format!(
                                    "{asker} asks{}",
                                    if app.choices.len() > 1 {
                                        format!(" · 1 of {}", app.choices.len())
                                    } else {
                                        String::new()
                                    }
                                )),
                        ),
                )
                .child(
                    div()
                        .id("choice-skip-popup")
                        .px(px(12.))
                        .h(px(28.))
                        .flex()
                        .items_center()
                        .rounded(px(7.))
                        .text_size(theme.ui(10.5))
                        .text_color(theme.overlay)
                        .cursor_pointer()
                        .hover(|s| s.text_color(theme.red))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.skip_choice();
                            cx.notify();
                        }))
                        .child("Skip"),
                )
                .child(
                    div()
                        .id("choice-later")
                        .px(px(12.))
                        .h(px(28.))
                        .flex()
                        .items_center()
                        .rounded(px(7.))
                        .border_1()
                        .border_color(theme.text.opacity(0.12))
                        .text_size(theme.ui(10.5))
                        .text_color(theme.subtext)
                        .cursor_pointer()
                        .hover(|s| s.bg(theme.surface0))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.choice_open = false;
                            this.save();
                            cx.notify();
                        }))
                        .child("Choose later"),
                ),
            crate::popup::CHOICE,
            size,
            cx,
        ))
        .child(
            div()
                .id("choice-body")
                .flex_1()
                .overflow_y_scroll()
                .p(px(18.))
                .flex()
                .flex_col()
                .gap(px(14.))
                .children((!choice.question.is_empty()).then(|| {
                    div()
                        .text_size(theme.ui(11.))
                        .text_color(theme.subtext)
                        .child(choice.question.clone())
                }))
                .child(
                    div()
                        .w_full()
                        .flex()
                        .flex_wrap()
                        .gap(px(12.))
                        .children(cards),
                ),
        )
        .child(
            div()
                .px(px(18.))
                .py(px(8.))
                .flex_shrink_0()
                .border_t_1()
                .border_color(theme.text.opacity(0.06))
                .flex()
                .items_center()
                .gap(px(5.))
                .text_size(theme.ui(9.5))
                .text_color(theme.overlay)
                .child(div().font_family(ICON_FONT).child(icons::CHECK))
                .child("Click an option or press its number · Esc to choose later"),
        );

    Some(crate::popup::frame(
        "choice-backdrop",
        crate::popup::CHOICE,
        app,
        card,
        |this| {
            this.choice_open = false;
            this.save();
        },
        cx,
    ))
}
