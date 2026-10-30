//! New Project dialog state and view (M17 follow-up extraction).
//!
//! The state stays UI-framework-free; the view methods only lay out the
//! existing `TextField` model and forward clicks to the editor actions.

use super::*;

/// How a new project starts (M16 slices 8–9, D-044).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NewProjectMode {
    Template,
    Ai,
}

/// Which dialog field owns typing. The title is the default; the AI prompt
/// takes over when the AI mode is selected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NewProjectField {
    Title,
    Prompt,
}

/// The New Project modal (M10 slice 2b): title, kind, primary language, the
/// destination folder and the native save-panel phase. Choosing a folder and
/// accepting are separate actions: the folder is only stored here, and
/// `accept_new_project` creates the template or starts the AI plan. `picking`
/// is true while the OS dialog owns the app, so duplicate panels cannot open.
/// The AI mode adds a prompt, a busy phase and a reviewable file plan.
#[derive(Debug)]
pub(crate) struct NewProjectState {
    pub(crate) title: TextField,
    /// Legacy enum used by the AI task/labels; custom ids map to `Other`.
    pub(crate) kind: ProjectType,
    /// Selected package id; may be a user-defined type.
    pub(crate) type_id: String,
    /// Installed types shown in the dialog (built-ins + user packages).
    pub(crate) types: Vec<ProjectTypePackage>,
    pub(crate) lang: Lang,
    pub(crate) mode: NewProjectMode,
    pub(crate) prompt: TextField,
    pub(crate) field: NewProjectField,
    pub(crate) error: String,
    pub(crate) picking: bool,
    /// AI generation in progress (the save panel has already returned).
    pub(crate) busy: bool,
    /// Destination folder chosen with the folder button. Nothing exists
    /// there until the user accepts.
    pub(crate) path: Option<PathBuf>,
    /// Reviewable AI plan; nothing is written until the user confirms.
    pub(crate) plan: Option<ScaffoldPlan>,
}

impl NewProjectState {
    pub(crate) fn text_field(&self, field: NewProjectField) -> &TextField {
        match field {
            NewProjectField::Title => &self.title,
            NewProjectField::Prompt => &self.prompt,
        }
    }

    pub(crate) fn text_field_mut(&mut self, field: NewProjectField) -> &mut TextField {
        match field {
            NewProjectField::Title => &mut self.title,
            NewProjectField::Prompt => &mut self.prompt,
        }
    }

    pub(crate) fn active_text_field_mut(&mut self) -> &mut TextField {
        self.text_field_mut(self.field)
    }
}

/// Snapshot of the dialog inputs handed to the AI scaffold worker: what the
/// user saw when pressing Accept decides the plan, even if the modal is still
/// on screen while the model runs.
#[derive(Debug, Clone)]
pub(crate) struct NewProjectDraft {
    pub(crate) title: String,
    pub(crate) kind: ProjectType,
    pub(crate) type_label: String,
    pub(crate) lang: Lang,
    pub(crate) prompt: String,
}

impl Editor {
    /// Render the New Project title or prompt with selection, caret and
    /// mouse drag. The model lives in `TextField`; this view only lays it out.
    fn new_project_text_input(
        state: &NewProjectState,
        field: NewProjectField,
        window: &Window,
        width: Pixels,
        min_height: Pixels,
        placeholder: &str,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let text_field = state.text_field(field);
        let focused = state.field == field && !state.picking && !state.busy && state.plan.is_none();
        let text = text_field.text().to_string();
        let selection = text_field.selection();
        let caret = text_field.caret();
        let line_height = px(theme::line_height());
        let rows = text_field.rows(window, f32::from(width));
        let mut column = div().flex().flex_col().w_full();
        if text.is_empty() {
            let mut row = div()
                .h(line_height)
                .flex()
                .items_center()
                .whitespace_nowrap();
            if focused {
                row = row.child(cursor_element());
            }
            if !placeholder.is_empty() {
                row = row.child(
                    div()
                        .text_color(rgb(theme::gutter_fg()))
                        .child(placeholder.to_string()),
                );
            }
            column = column.child(row);
        } else {
            // Draw the caret in exactly one row. At a soft-wrap boundary the
            // same byte offset is the end of one row and the start of the
            // next; prefer the row where the caret has room, otherwise the
            // last row for the document end.
            let caret_row = rows
                .iter()
                .position(|row| caret >= row.start && (caret < row.end || row.end == text.len()))
                .unwrap_or(rows.len().saturating_sub(1));
            for (row_index, row) in rows.iter().enumerate() {
                let row = *row;
                let mut cuts = vec![row.start, row.end];
                if selection.end > row.start && selection.start < row.end {
                    cuts.push(selection.start.max(row.start).min(row.end));
                    cuts.push(selection.end.max(row.start).min(row.end));
                }
                if focused && row_index == caret_row && caret >= row.start && caret <= row.end {
                    cuts.push(caret);
                }
                cuts.sort_unstable();
                cuts.dedup();
                let mut visual = div()
                    .h(line_height)
                    .flex()
                    .items_center()
                    .whitespace_nowrap()
                    .overflow_hidden();
                for pair in cuts.windows(2) {
                    let (start, end) = (pair[0], pair[1]);
                    if focused && row_index == caret_row && start == caret {
                        visual = visual.child(cursor_element());
                    }
                    if start == end {
                        continue;
                    }
                    let selected = start >= selection.start && end <= selection.end;
                    let mut segment = div().whitespace_nowrap();
                    if selected {
                        segment = segment.bg(rgba(theme::selection_bg()));
                    }
                    visual = visual.child(segment.child(text[start..end].to_string()));
                }
                if focused && row_index == caret_row && caret == row.end && caret != row.start {
                    visual = visual.child(cursor_element());
                }
                column = column.child(visual);
            }
        }

        let bounds_cell: Rc<Cell<Option<Bounds<Pixels>>>> = Rc::new(Cell::new(None));
        let bounds_for_canvas = bounds_cell.clone();
        let bounds_for_click = bounds_cell.clone();
        let bounds_for_drag = bounds_cell.clone();
        let field_for_click = field;
        let field_for_drag = field;
        let width_for_click = width;
        let width_for_drag = width;
        let id = match field {
            NewProjectField::Title => "new-project-title-input",
            NewProjectField::Prompt => "new-project-prompt-input",
        };
        div()
            .id(id)
            .relative()
            .w_full()
            .p_2()
            .bg(rgb(theme::bg()))
            .border_1()
            .border_color(rgb(if focused {
                theme::default_fg()
            } else {
                theme::explorer_border()
            }))
            .font_family(FONT_FAMILY)
            .text_size(px(theme::text_size()))
            .line_height(line_height)
            .text_color(rgb(theme::default_fg()))
            .child(
                div()
                    .relative()
                    .w_full()
                    .min_h(min_height)
                    .cursor_text()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                            let Some(state) = this.new_project.as_mut() else {
                                return;
                            };
                            if state.picking || state.busy || state.plan.is_some() {
                                return;
                            }
                            state.field = field_for_click;
                            state.error.clear();
                            let Some(bounds) = bounds_for_click.get() else {
                                return;
                            };
                            let index = state.text_field(field_for_click).index_for_point(
                                window,
                                event.position,
                                bounds,
                                f32::from(width_for_click),
                            );
                            state
                                .text_field_mut(field_for_click)
                                .begin_drag(index, event.modifiers.shift);
                            cx.notify();
                        }),
                    )
                    .on_mouse_move(
                        cx.listener(move |this, event: &MouseMoveEvent, window, cx| {
                            if event.pressed_button != Some(MouseButton::Left) {
                                return;
                            }
                            let Some(state) = this.new_project.as_mut() else {
                                return;
                            };
                            if !state.text_field(field_for_drag).is_dragging() {
                                return;
                            }
                            let Some(bounds) = bounds_for_drag.get() else {
                                return;
                            };
                            let index = state.text_field(field_for_drag).index_for_point(
                                window,
                                event.position,
                                bounds,
                                f32::from(width_for_drag),
                            );
                            state.text_field_mut(field_for_drag).drag_to(index);
                            cx.notify();
                        }),
                    )
                    .on_mouse_up(
                        MouseButton::Left,
                        cx.listener(|this, _, _, cx| {
                            if let Some(state) = this.new_project.as_mut() {
                                let field = state.field;
                                state.text_field_mut(field).end_drag();
                                cx.notify();
                            }
                        }),
                    )
                    .on_mouse_up_out(
                        MouseButton::Left,
                        cx.listener(|this, _, _, cx| {
                            if let Some(state) = this.new_project.as_mut() {
                                let field = state.field;
                                state.text_field_mut(field).end_drag();
                                cx.notify();
                            }
                        }),
                    )
                    .child(column),
            )
            .child(
                canvas(
                    move |bounds: Bounds<Pixels>, _window, _cx| {
                        let content = Bounds::new(
                            point(
                                bounds.left() + px(ROW_PADDING_X),
                                bounds.top() + px(ROW_PADDING_X),
                            ),
                            size(
                                (bounds.size.width - px(ROW_PADDING_X * 2.0)).max(px(0.0)),
                                (bounds.size.height - px(ROW_PADDING_X * 2.0)).max(px(0.0)),
                            ),
                        );
                        bounds_for_canvas.set(Some(content));
                    },
                    |_bounds, _state, _window, _cx| {},
                )
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
            )
            .into_any_element()
    }

    /// New Project dialog: title, kind, primary language, destination folder,
    /// project base and review. The folder button opens the native panel;
    /// Enter accepts (creating the template or starting the AI plan). Esc
    /// closes the dialog.
    pub(crate) fn new_project_overlay(&self, window: &Window, cx: &mut Context<Self>) -> gpui::Div {
        let state = self.new_project.as_ref().expect("overlay only when open");
        let editable = !state.picking && !state.busy && state.plan.is_none();
        let mut modes = div().flex().flex_wrap().gap_2();
        for (mode, key) in [
            (NewProjectMode::Template, "newproject.mode.template"),
            (NewProjectMode::Ai, "newproject.mode.ai"),
        ] {
            let selected = state.mode == mode;
            let enabled = editable && (mode != NewProjectMode::Ai || self.ai_scaffold_available());
            let mut chip = div()
                .id(key)
                .px_3()
                .py_1()
                .bg(rgb(if selected {
                    theme::doc_selected_bg()
                } else {
                    theme::explorer_bg()
                }))
                .border_1()
                .border_color(rgb(if selected {
                    theme::default_fg()
                } else {
                    theme::explorer_border()
                }))
                .text_color(rgb(if enabled || selected {
                    theme::default_fg()
                } else {
                    theme::gutter_fg()
                }))
                .when(enabled, |chip| chip.cursor_pointer())
                .child(self.t(key));
            if enabled {
                chip = chip.on_click(cx.listener(move |this, _, _, cx| {
                    if let Some(state) = this.new_project.as_mut()
                        && !state.busy
                        && state.plan.is_none()
                    {
                        state.mode = mode;
                        state.field = if mode == NewProjectMode::Ai {
                            NewProjectField::Prompt
                        } else {
                            NewProjectField::Title
                        };
                        state.error.clear();
                    }
                    cx.notify();
                }));
            }
            modes = modes.child(chip);
        }
        let mut kinds = div().flex().flex_wrap().gap_2();
        for (index, package) in state.types.iter().enumerate() {
            let selected = package.id == state.type_id;
            let kind = ProjectType::parse(&package.id);
            let label = kind.map_or_else(
                || package.name.clone(),
                |kind| self.t(new_project::type_i18n_key(kind)).to_string(),
            );
            let package_id = package.id.clone();
            let package_kind = kind.unwrap_or(ProjectType::Other);
            kinds = kinds.child(
                div()
                    .id(("project-type-chip", index as u64))
                    .px_3()
                    .py_1()
                    .bg(rgb(if selected {
                        theme::doc_selected_bg()
                    } else {
                        theme::explorer_bg()
                    }))
                    .border_1()
                    .border_color(rgb(if selected {
                        theme::default_fg()
                    } else {
                        theme::explorer_border()
                    }))
                    .text_color(rgb(if selected {
                        theme::default_fg()
                    } else {
                        theme::status_fg()
                    }))
                    .cursor_pointer()
                    .child(label)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(state) = this.new_project.as_mut()
                            && !state.picking
                            && !state.busy
                            && state.plan.is_none()
                        {
                            state.type_id = package_id.clone();
                            state.kind = package_kind;
                            state.error.clear();
                        }
                        cx.notify();
                    })),
            );
        }
        let mut languages = div().flex().gap_2();
        for lang in [Lang::En, Lang::Es] {
            let selected = lang == state.lang;
            languages = languages.child(
                div()
                    .id(lang.key())
                    .px_3()
                    .py_1()
                    .bg(rgb(if selected {
                        theme::doc_selected_bg()
                    } else {
                        theme::explorer_bg()
                    }))
                    .border_1()
                    .border_color(rgb(if selected {
                        theme::default_fg()
                    } else {
                        theme::explorer_border()
                    }))
                    .text_color(rgb(if selected {
                        theme::default_fg()
                    } else {
                        theme::status_fg()
                    }))
                    .cursor_pointer()
                    .child(lang.name())
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(state) = this.new_project.as_mut()
                            && !state.picking
                            && !state.busy
                            && state.plan.is_none()
                        {
                            state.lang = lang;
                            state.error.clear();
                        }
                        cx.notify();
                    })),
            );
        }
        let type_summary = state
            .types
            .iter()
            .find(|package| package.id == state.type_id)
            .map(|package| {
                format!(
                    "{} · {} carpetas · {} archivos",
                    package.description,
                    package.directories.len(),
                    package.files.len()
                )
            })
            .unwrap_or_default();
        div()
            .absolute()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .bg(rgba(0x00000099))
            .on_mouse_down(MouseButton::Left, cx.listener(|_, _, _, _| {}))
            .on_mouse_up(MouseButton::Left, cx.listener(|_, _, _, _| {}))
            .child(
                div()
                    .w(px(600.0))
                    .flex()
                    .flex_col()
                    .gap_3()
                    .p_4()
                    .bg(rgb(theme::explorer_bg()))
                    .border_1()
                    .border_color(rgb(theme::explorer_border()))
                    .child(
                        div()
                            .text_color(rgb(theme::default_fg()))
                            .child(self.t("newproject.title")),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .text_color(rgb(theme::status_fg()))
                                    .child(self.t("newproject.name")),
                            )
                            .child(div().flex_1().child(Self::new_project_text_input(
                                state,
                                NewProjectField::Title,
                                window,
                                px(480.0),
                                px(theme::line_height()),
                                "",
                                cx,
                            ))),
                    )
                    .child(
                        div()
                            .flex()
                            .items_start()
                            .gap_2()
                            .child(
                                div()
                                    .pt_1()
                                    .text_color(rgb(theme::status_fg()))
                                    .child(self.t("newproject.type")),
                            )
                            .child(kinds),
                    )
                    .when(!type_summary.is_empty(), |card| {
                        card.child(
                            div()
                                .text_xs()
                                .text_color(rgb(theme::gutter_fg()))
                                .child(type_summary),
                        )
                    })
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .text_color(rgb(theme::status_fg()))
                                    .child(self.t("newproject.language")),
                            )
                            .child(languages),
                    )
                    .child({
                        let folder = state
                            .path
                            .as_ref()
                            .map(|path| path.display().to_string())
                            .unwrap_or_else(|| self.t("newproject.no_folder").to_string());
                        let folder_enabled = !state.picking && !state.busy && state.plan.is_none();
                        let mut button = div()
                            .id("new-project-folder")
                            .flex_none()
                            .px_3()
                            .py_1()
                            .bg(rgb(if folder_enabled {
                                theme::status_bg()
                            } else {
                                theme::explorer_bg()
                            }))
                            .text_color(rgb(if folder_enabled {
                                theme::default_fg()
                            } else {
                                theme::gutter_fg()
                            }))
                            .child(self.t("newproject.choose_folder"));
                        if folder_enabled {
                            button = button.cursor_pointer().on_click(cx.listener(
                                |this, _, window, cx| this.choose_new_project_folder(window, cx),
                            ));
                        }
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .text_color(rgb(theme::status_fg()))
                                    .child(self.t("newproject.folder")),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .overflow_hidden()
                                    .whitespace_nowrap()
                                    .text_xs()
                                    .text_color(rgb(if state.path.is_some() {
                                        theme::default_fg()
                                    } else {
                                        theme::gutter_fg()
                                    }))
                                    .child(folder),
                            )
                            .child(button)
                    })
                    .child(
                        div()
                            .flex()
                            .items_start()
                            .gap_2()
                            .child(
                                div()
                                    .pt_1()
                                    .text_color(rgb(theme::status_fg()))
                                    .child(self.t("newproject.mode")),
                            )
                            .child(modes),
                    )
                    .when(!self.ai_scaffold_available(), |card| {
                        card.child(
                            div()
                                .text_xs()
                                .text_color(rgb(theme::gutter_fg()))
                                .child(self.t("newproject.ai_unavailable")),
                        )
                    })
                    .when(
                        state.mode == NewProjectMode::Ai && self.ai_scaffold_available(),
                        |card| {
                            card.child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap_1()
                                    .child(
                                        div()
                                            .text_color(rgb(theme::status_fg()))
                                            .child(self.t("newproject.prompt")),
                                    )
                                    .child(Self::new_project_text_input(
                                        state,
                                        NewProjectField::Prompt,
                                        window,
                                        px(552.0),
                                        px(60.0),
                                        self.t("newproject.prompt_hint"),
                                        cx,
                                    )),
                            )
                        },
                    )
                    .when(state.plan.is_some(), |card| {
                        let plan = state.plan.as_ref().expect("checked");
                        let mut list = div()
                            .id("new-project-plan")
                            .max_h(px(180.0))
                            .overflow_y_scroll()
                            .flex()
                            .flex_col()
                            .border_1()
                            .border_color(rgb(theme::explorer_border()));
                        for file in &plan.files {
                            let preview: String =
                                file.content.lines().take(2).collect::<Vec<_>>().join(" ");
                            let preview: String = preview.chars().take(160).collect();
                            list = list.child(
                                div()
                                    .p_2()
                                    .border_b_1()
                                    .border_color(rgb(theme::explorer_border()))
                                    .child(
                                        div()
                                            .text_color(rgb(theme::default_fg()))
                                            .child(file.path.clone()),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(rgb(theme::gutter_fg()))
                                            .child(preview),
                                    ),
                            );
                        }
                        card.child(list)
                    })
                    .child(
                        div()
                            .text_color(rgb(theme::status_fg()))
                            .child(if state.picking {
                                self.t("newproject.waiting")
                            } else if state.busy {
                                self.t("newproject.generating")
                            } else if state.plan.is_some() {
                                self.t("newproject.plan")
                            } else if state.path.is_none() {
                                self.t("newproject.folder_hint")
                            } else {
                                self.t("newproject.hint")
                            }),
                    )
                    .when(!state.error.is_empty(), |card| {
                        card.child(
                            div()
                                .text_color(rgb(theme::agent_err_fg()))
                                .child(state.error.clone()),
                        )
                    })
                    .child({
                        let plan_ready = state.plan.is_some();
                        let can_click = !state.picking && !state.busy && state.path.is_some();
                        let label = if plan_ready || state.mode == NewProjectMode::Template {
                            self.t("newproject.create")
                        } else {
                            self.t("newproject.generate")
                        };
                        let mut primary = div()
                            .id("new-project-create")
                            .px_3()
                            .py_1()
                            .bg(rgb(if can_click {
                                theme::status_bg()
                            } else {
                                theme::explorer_bg()
                            }))
                            .text_color(rgb(if can_click {
                                theme::default_fg()
                            } else {
                                theme::gutter_fg()
                            }))
                            .child(label);
                        if can_click {
                            primary =
                                primary
                                    .cursor_pointer()
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.accept_new_project(cx);
                                    }));
                        }
                        div().flex().gap_2().child(primary).child(
                            div()
                                .id("new-project-cancel")
                                .px_3()
                                .py_1()
                                .bg(rgb(theme::status_bg()))
                                .text_color(rgb(theme::status_fg()))
                                .cursor_pointer()
                                .child(self.t("newproject.cancel"))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    if !this.new_project.as_ref().is_some_and(|state| state.picking)
                                    {
                                        this.new_project = None;
                                        this.new_project_generation =
                                            this.new_project_generation.saturating_add(1);
                                    }
                                    cx.notify();
                                })),
                        )
                    }),
            )
    }
}
