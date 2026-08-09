# KanbanBoard - Trello-like Kanban clone, PythScribe canonical track.
#
# Dual-track with KanbanBoard.tsx (React oracle). Renders identical DOM for
# identical state (see KanbanBoard.test.tsx render-parity suite).
#
# Deliberate stress surface: card drag-and-drop is implemented with RAW
# POINTER EVENTS (pointerdown / window pointermove / pointerup, Escape
# cancels) - no dnd library. A floating ghost follows the pointer, the
# hovered column gets a dropzone highlight, and drops reorder within a
# column or move across columns. Board state persists to localStorage;
# the Reset button restores the fixtures (e2e determinism hook).
#
# NOTE: `#` comment block, not a triple-quoted module docstring - see
# CONTRIBUTING.md "Known friction" (Turbopack UTF-8 char-boundary panic).
#
# Compiler-bug workaround (documented in the build report): isinstance(x, list)
# emits `__pyIsInstance(x, list)` with `list` unbound in the emitted JS -
# ReferenceError at runtime. We call Array.isArray(x) directly instead.
"use client"

import "./KanbanBoard.css"

from pyths.react import component, use_effect, use_ref, use_state
from .fixtures import KANBAN_FIXTURE, KANBAN_STORAGE_KEY


def next_card_id(columns):
    n = 0
    for col in columns:
        for card in col["cards"]:
            v = int(card["id"][1:])
            if v > n:
                n = v
    return "c" + str(n + 1)


def next_col_id(columns):
    n = 0
    for col in columns:
        v = int(col["id"][3:])
        if v > n:
            n = v
    return "col" + str(n + 1)


# Hit-test the pointer position against the rendered columns: returns the
# hovered column id plus the insertion index among that column's cards
# (the dragged card itself is excluded from the count, matching the
# remove-then-insert order move_card applies on drop).
def hit_test(x, y, dragged_id):
    el = document.elementFromPoint(x, y)
    if el is None:
        return [None, 0]
    col_el = el.closest("[data-col-id]")
    if col_el is None:
        return [None, 0]
    col_id = col_el.getAttribute("data-col-id")
    cards = col_el.querySelectorAll("[data-card-id]")
    idx = 0
    for card_el in cards:
        if card_el.getAttribute("data-card-id") == dragged_id:
            continue
        r = card_el.getBoundingClientRect()
        if y < r.top + r.height / 2:
            break
        idx += 1
    return [col_id, idx]


def move_card(columns, card_id, to_col, index):
    card = None
    for col in columns:
        for c in col["cards"]:
            if c["id"] == card_id:
                card = c
    if card is None:
        return columns
    without = [{**col, "cards": [c for c in col["cards"] if c["id"] != card_id]} for col in columns]
    return [
        {**col, "cards": [*col["cards"][:index], card, *col["cards"][index:]]} if col["id"] == to_col else col
        for col in without
    ]


@c
def KanbanBoard():
    columns, set_columns = us(lambda: structuredClone(KANBAN_FIXTURE))
    drag, set_drag = us(None)
    editing, set_editing = us(None)
    composing, set_composing = us(None)
    compose_text, set_compose_text = us("")
    renaming, set_renaming = us(None)
    rename_text, set_rename_text = us("")
    adding_col, set_adding_col = us(False)
    new_col_text, set_new_col_text = us("")
    did_drag_ref = ur(False)

    # localStorage persistence - load once after mount (client-only, so the
    # Next SSR pass and hydration both render the fixtures). Saving is
    # WRITE-THROUGH at each mutation site (update_board) rather than a
    # [columns]-effect: a save-effect's mount run closes over the initial
    # fixtures and clobbers the saved board before the load's re-render
    # lands (guaranteed under StrictMode's double-effect pass).
    def _load():
        try:
            raw = localStorage.getItem(KANBAN_STORAGE_KEY)
            if raw is not None:
                saved = JSON.parse(raw)
                if Array.isArray(saved):
                    set_columns(saved)
        except Exception:
            pass

    ue(_load, [])

    def update_board(next_columns):
        set_columns(next_columns)
        localStorage.setItem(KANBAN_STORAGE_KEY, JSON.stringify(next_columns))

    # Pointer-event drag machine. Listeners live on window for the whole
    # drag so fast pointer moves can't escape the card element. Reattached
    # per state change (deps) - cheap, and keeps the handlers closure-fresh.
    def _drag_effect():
        if drag is None:
            return lambda: None

        def on_move(e):
            if not drag["active"]:
                dx = e.clientX - drag["start_x"]
                dy = e.clientY - drag["start_y"]
                if dx * dx + dy * dy < 25:
                    return
                did_drag_ref.current = True
            e.preventDefault()
            over_col, over_index = hit_test(e.clientX, e.clientY, drag["card_id"])
            set_drag({**drag, "active": True, "x": e.clientX, "y": e.clientY, "over_col": over_col, "over_index": over_index})

        def on_up(e):
            if drag["active"] and drag["over_col"] is not None:
                update_board(move_card(columns, drag["card_id"], drag["over_col"], drag["over_index"]))
            set_drag(None)

        def on_key(e):
            if e.key == "Escape":
                set_drag(None)

        window.addEventListener("pointermove", on_move)
        window.addEventListener("pointerup", on_up)
        window.addEventListener("keydown", on_key)

        def _cleanup():
            window.removeEventListener("pointermove", on_move)
            window.removeEventListener("pointerup", on_up)
            window.removeEventListener("keydown", on_key)

        return _cleanup

    ue(_drag_effect, [drag, columns])

    def on_card_pointer_down(e, card, col_id):
        if e.button != 0:
            return
        if e.target.closest("button, textarea, input"):
            return
        rect = e.currentTarget.getBoundingClientRect()
        did_drag_ref.current = False
        set_drag({
            "card_id": card["id"],
            "from_col": col_id,
            "text": card["text"],
            "start_x": e.clientX,
            "start_y": e.clientY,
            "x": e.clientX,
            "y": e.clientY,
            "offset_x": e.clientX - rect.left,
            "offset_y": e.clientY - rect.top,
            "width": rect.width,
            "active": False,
            "over_col": None,
            "over_index": 0,
        })

    def on_card_click(card):
        if did_drag_ref.current:
            did_drag_ref.current = False
            return
        set_editing({"card_id": card["id"], "text": card["text"]})

    def save_edit():
        if editing is None:
            return
        text = editing["text"].strip()
        if text:
            update_board([
                {**col, "cards": [{**c, "text": text} if c["id"] == editing["card_id"] else c for c in col["cards"]]}
                for col in columns
            ])
        set_editing(None)

    def on_edit_key(e):
        if e.key == "Enter" and not e.shiftKey:
            e.preventDefault()
            save_edit()
        elif e.key == "Escape":
            set_editing(None)

    def delete_card(e, card_id):
        e.stopPropagation()
        update_board([{**col, "cards": [c for c in col["cards"] if c["id"] != card_id]} for col in columns])

    def add_card(col_id):
        text = compose_text.strip()
        if not text:
            return
        cid = next_card_id(columns)
        update_board([
            {**col, "cards": [*col["cards"], {"id": cid, "text": text}]} if col["id"] == col_id else col
            for col in columns
        ])
        set_compose_text("")

    def on_compose_key(e, col_id):
        if e.key == "Enter" and not e.shiftKey:
            e.preventDefault()
            add_card(col_id)
        elif e.key == "Escape":
            set_composing(None)
            set_compose_text("")

    def start_rename(col):
        set_renaming(col["id"])
        set_rename_text(col["title"])

    def save_rename():
        text = rename_text.strip()
        if text:
            update_board([{**col, "title": text} if col["id"] == renaming else col for col in columns])
        set_renaming(None)

    def on_rename_key(e):
        if e.key == "Enter":
            e.preventDefault()
            save_rename()
        elif e.key == "Escape":
            set_renaming(None)

    def add_column():
        text = new_col_text.strip()
        if not text:
            return
        update_board([*columns, {"id": next_col_id(columns), "title": text, "cards": []}])
        set_new_col_text("")
        set_adding_col(False)

    def on_new_col_key(e):
        if e.key == "Enter":
            e.preventDefault()
            add_column()
        elif e.key == "Escape":
            set_adding_col(False)
            set_new_col_text("")

    def _cancel_compose():
        set_composing(None)
        set_compose_text("")

    def _open_compose(col_id):
        set_composing(col_id)
        set_compose_text("")

    def _cancel_new_col():
        set_adding_col(False)
        set_new_col_text("")

    def reset_board():
        update_board(structuredClone(KANBAN_FIXTURE))
        set_drag(None)
        set_editing(None)
        set_composing(None)
        set_compose_text("")
        set_renaming(None)
        set_adding_col(False)
        set_new_col_text("")

    return div(
        cn="kb-board" + (" kb-board--dragging" if drag is not None and drag["active"] else ""),
        data_testid="kanban-board",
        div(
            cn="kb-toolbar",
            h1(cn="kb-title", "Kanban"),
            button(cn="kb-reset", data_testid="kb-reset", oc=reset_board, "Reset board"),
        ),
        div(
            cn="kb-cols",
            data_testid="kb-cols",
            [
                section(
                    key=col["id"],
                    cn="kb-col" + (" kb-col--drop" if drag is not None and drag["active"] and drag["over_col"] == col["id"] else ""),
                    data_col_id=col["id"],
                    data_testid="kb-col-" + col["id"],
                    header(
                        cn="kb-col-head",
                        input(
                            cn="kb-rename",
                            data_testid="kb-rename-input",
                            value=rename_text,
                            auto_focus=True,
                            oh=lambda e: set_rename_text(e.target.value),
                            on_key_down=on_rename_key,
                        ) if renaming == col["id"] else h2(
                            cn="kb-col-title",
                            data_testid="kb-col-title-" + col["id"],
                            oc=lambda: start_rename(col),
                            col["title"],
                        ),
                        span(cn="kb-count", data_testid="kb-count-" + col["id"], len(col["cards"])),
                    ),
                    div(
                        cn="kb-cards",
                        [
                            div(
                                key=card["id"],
                                cn="kb-card kb-card--editing",
                                data_card_id=card["id"],
                                textarea(
                                    cn="kb-edit",
                                    data_testid="kb-edit-input",
                                    value=editing["text"],
                                    auto_focus=True,
                                    oh=lambda e: set_editing({"card_id": card["id"], "text": e.target.value}),
                                    on_key_down=on_edit_key,
                                ),
                            ) if editing is not None and editing["card_id"] == card["id"] else div(
                                key=card["id"],
                                cn="kb-card" + (" kb-card--dragging" if drag is not None and drag["active"] and drag["card_id"] == card["id"] else ""),
                                data_card_id=card["id"],
                                data_testid="kb-card-" + card["id"],
                                on_pointer_down=lambda e: on_card_pointer_down(e, card, col["id"]),
                                oc=lambda: on_card_click(card),
                                div(cn="kb-card-text", card["text"]),
                                button(cn="kb-del", data_testid="kb-del-" + card["id"], oc=lambda e: delete_card(e, card["id"]), "×"),
                            )
                            for card in col["cards"]
                        ],
                    ),
                    div(
                        cn="kb-col-foot",
                        div(
                            cn="kb-composer",
                            data_testid="kb-composer",
                            textarea(
                                cn="kb-compose",
                                data_testid="kb-compose-input",
                                ph="Enter a card title",
                                value=compose_text,
                                auto_focus=True,
                                oh=lambda e: set_compose_text(e.target.value),
                                on_key_down=lambda e: on_compose_key(e, col["id"]),
                            ),
                            div(
                                cn="kb-composer-actions",
                                button(cn="kb-compose-add", data_testid="kb-compose-add", oc=lambda: add_card(col["id"]), "Add card"),
                                button(cn="kb-compose-cancel", data_testid="kb-compose-cancel", oc=_cancel_compose, "Cancel"),
                            ),
                        ) if composing == col["id"] else button(
                            cn="kb-add-card",
                            data_testid="kb-add-card-" + col["id"],
                            oc=lambda: _open_compose(col["id"]),
                            "+ Add card",
                        ),
                    ),
                )
                for col in columns
            ],
            section(
                cn="kb-col kb-col--new",
                div(
                    cn="kb-newcol",
                    data_testid="kb-newcol",
                    input(
                        cn="kb-newcol-input",
                        data_testid="kb-newcol-input",
                        ph="Column title",
                        value=new_col_text,
                        auto_focus=True,
                        oh=lambda e: set_new_col_text(e.target.value),
                        on_key_down=on_new_col_key,
                    ),
                    div(
                        cn="kb-composer-actions",
                        button(cn="kb-newcol-add", data_testid="kb-newcol-add", oc=add_column, "Add column"),
                        button(cn="kb-newcol-cancel", data_testid="kb-newcol-cancel", oc=_cancel_new_col, "Cancel"),
                    ),
                ) if adding_col else button(
                    cn="kb-add-col",
                    data_testid="kb-add-col",
                    oc=lambda: set_adding_col(True),
                    "+ Add column",
                ),
            ),
        ),
        drag is not None and drag["active"] and div(
            cn="kb-ghost",
            data_testid="kb-ghost",
            st={"left": drag["x"] - drag["offset_x"], "top": drag["y"] - drag["offset_y"], "width": drag["width"]},
            drag["text"],
        ),
    )


__default__ = KanbanBoard
