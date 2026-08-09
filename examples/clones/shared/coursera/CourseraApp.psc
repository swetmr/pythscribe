# Coursera clone - PythScribe canonical track. Dual-track with
# CourseraApp.tsx (the React oracle); CourseraApp.psc round-trips to this
# file byte-identically (`pyths expand --verify`). All three tracks render
# identical DOM for the same fixtures (see CourseraApp.test.tsx).
#
# Components: CourseraApp (client-side view switch) -> Catalog (text search
# + category filter chips, combined) and CourseDetail (syllabus accordion,
# enroll banner, modules-completed progress bar) -> QuizBoundary (a real
# PythScribe CLASS component error boundary - `class QuizBoundary(Component)`
# with componentDidCatch + @staticmethod getDerivedStateFromError; exercises
# the A3 native-base-constructor fix) -> Quiz (graded radio / checkbox /
# short-text quiz: per-question validation, scoring, per-question review,
# retake, and a hidden dev-only crash trigger the e2e uses to prove the
# boundary catches a render throw).
#
# NOTE: `#` comment block, not a triple-quoted module docstring - avoids the
# Next.js 16 Turbopack UTF-8 char-boundary panic on non-ASCII docstrings
# (see CONTRIBUTING.md "Known friction").
"use client"

import "./CourseraApp.css"

from pyths.react import component, psx, use_state
from react import Component
from .fixtures import COURSES, QUIZ


def _slug(s):
    return s.lower().replace(" ", "-")


# Tier-7 custom exception, dual-tracked with `class QuizCrashError extends
# Error` in CourseraApp.tsx. Both tracks surface the deliberate dev crash
# identically as "QuizCrashError: quiz dev crash" (the runtime stamps
# `.name` from the Python class name), so the e2e interaction differential
# enforces exact error identity - name + message - instead of allowlisting
# a name-blind message substring. (The old `_crash` helper worked around a
# since-fixed PSX mis-lowering of capitalized calls in @component bodies;
# `raise` now lowers correctly inline.)
class QuizCrashError(Exception):
    pass


@c
def Catalog(on_select):
    query, set_query = us("")
    category, set_category = us("All")
    cats = ["All"]
    for c in COURSES:
        if c["category"] not in cats:
            cats.append(c["category"])
    q = query.strip().lower()
    filtered = [c for c in COURSES if (q == "" or q in c["title"].lower()) and (category == "All" or c["category"] == category)]
    return section(
        cn="cx-catalog",
        data_testid="catalog",
        h1("Explore courses"),
        input(
            cn="cx-search",
            data_testid="search-input",
            type="text",
            ph="Search courses",
            value=query,
            oh=lambda e: set_query(e.target.value),
        ),
        div(
            cn="cx-chips",
            *[
                button(
                    key=cat,
                    cn="cx-chip",
                    data_testid="chip-" + _slug(cat),
                    data_active=cat == category,
                    oc=lambda: set_category(cat),
                    cat,
                )
                for cat in cats
            ],
        ),
        p(cn="cx-count", data_testid="catalog-count", str(len(filtered)) + " courses"),
        len(filtered) == 0 and p(cn="cx-empty", data_testid="catalog-empty", "No courses match your filters."),
        div(
            cn="cx-grid",
            *[
                article(
                    key=c["id"],
                    cn="cx-card",
                    data_testid="course-card-" + c["id"],
                    oc=lambda: on_select(c["id"]),
                    span(cn="cx-cat-tag", c["category"]),
                    h2(c["title"]),
                    p(cn="cx-inst", c["instructor"]),
                )
                for c in filtered
            ],
        ),
    )


@c
def Quiz():
    answers, set_answers = us({})
    errors, set_errors = us([])
    submitted, set_submitted = us(False)
    crashed, set_crashed = us(False)
    if crashed:
        raise QuizCrashError("quiz dev crash")

    def _val(qid):
        return answers[qid] if qid in answers else None

    def _set_radio(qid, opt):
        set_answers({**answers, qid: opt})

    def _toggle_check(qid, opt):
        cur = _val(qid) if _val(qid) is not None else []
        if opt in cur:
            nxt = [o for o in cur if o != opt]
        else:
            nxt = cur + [opt]
        set_answers({**answers, qid: nxt})

    def _set_text(qid, text):
        set_answers({**answers, qid: text})

    def _answered(q):
        v = _val(q["id"])
        if q["kind"] == "checkbox":
            return v is not None and len(v) > 0
        if q["kind"] == "text":
            return v is not None and v.strip() != ""
        return v is not None

    def _same_set(a, b):
        if len(a) != len(b):
            return False
        for x in a:
            if x not in b:
                return False
        return True

    def _correct(q):
        v = _val(q["id"])
        if q["kind"] == "checkbox":
            return _same_set(v if v is not None else [], q["answer"])
        if q["kind"] == "text":
            return (v if v is not None else "").strip().lower() == q["answer"].lower()
        return v == q["answer"]

    def _submit():
        missing = [q["id"] for q in QUIZ["questions"] if not _answered(q)]
        set_errors(missing)
        if len(missing) == 0:
            set_submitted(True)

    def _retake():
        set_answers({})
        set_errors([])
        set_submitted(False)

    if submitted:
        score = len([q for q in QUIZ["questions"] if _correct(q)])
        return div(
            cn="cx-quiz",
            data_testid="quiz-score",
            h3(
                data_testid="quiz-score-line",
                "You scored " + str(score) + "/" + str(len(QUIZ["questions"])),
            ),
            ul(
                cn="cx-review",
                *[
                    li(
                        key=q["id"],
                        data_testid="quiz-review-" + q["id"],
                        data_correct=_correct(q),
                        ("Correct: " if _correct(q) else "Incorrect: ") + q["prompt"],
                    )
                    for q in QUIZ["questions"]
                ],
            ),
            button(cn="cx-btn", data_testid="quiz-retake", oc=_retake, "Retake quiz"),
        )

    return div(
        cn="cx-quiz",
        data_testid="quiz",
        *[
            fieldset(
                key=q["id"],
                cn="cx-q",
                data_testid="quiz-q-" + q["id"],
                legend(str(idx + 1) + ". " + q["prompt"]),
                q["kind"] == "radio" and [
                    label(
                        key=opt,
                        cn="cx-opt",
                        input(
                            type="radio",
                            name=q["id"],
                            value=opt,
                            checked=_val(q["id"]) == opt,
                            oh=lambda: _set_radio(q["id"], opt),
                        ),
                        opt,
                    )
                    for opt in q["options"]
                ],
                q["kind"] == "checkbox" and [
                    label(
                        key=opt,
                        cn="cx-opt",
                        input(
                            type="checkbox",
                            name=q["id"],
                            value=opt,
                            checked=_val(q["id"]) is not None and opt in _val(q["id"]),
                            oh=lambda: _toggle_check(q["id"], opt),
                        ),
                        opt,
                    )
                    for opt in q["options"]
                ],
                q["kind"] == "text" and input(
                    cn="cx-text-answer",
                    data_testid="quiz-text-" + q["id"],
                    type="text",
                    ph="Your answer",
                    value=_val(q["id"]) if _val(q["id"]) is not None else "",
                    oh=lambda e: _set_text(q["id"], e.target.value),
                ),
                q["id"] in errors and p(
                    cn="cx-q-err",
                    data_testid="quiz-q-err-" + q["id"],
                    "Please answer this question.",
                ),
            )
            for idx, q in enumerate(QUIZ["questions"])
        ],
        len(errors) > 0 and p(
            cn="cx-quiz-err",
            data_testid="quiz-error",
            "Answer all questions before submitting.",
        ),
        div(
            cn="cx-quiz-actions",
            button(cn="cx-btn", data_testid="quiz-submit", oc=_submit, "Submit quiz"),
            button(
                cn="cx-devcrash",
                data_testid="quiz-crash-dev",
                aria_hidden="true",
                tab_index=-1,
                oc=lambda: set_crashed(True),
                "dev: crash quiz",
            ),
        ),
    )


@psx
def _quiz_fallback(on_reload):
    return div(
        cn="cx-quiz-fallback",
        data_testid="quiz-fallback",
        p("The quiz crashed."),
        button(cn="cx-btn", data_testid="quiz-reload", oc=on_reload, "Reload quiz"),
    )


# The A3 exercise: a PythScribe class component extending React's native
# Component base. Native-constructor emission (no cooperative PyObject MRO
# wrap) is what keeps `self.state` alive here - see pyths_codegen_js
# test_class_extending_external_base_uses_native_constructor.
class QuizBoundary(Component):
    def __init__(self, props):
        super().__init__(props)
        self.state = {"hasError": False}

    @staticmethod
    def getDerivedStateFromError(error):
        return {"hasError": True}

    def componentDidCatch(self, error, info):
        pass

    def render(self):
        if self.state["hasError"]:
            return _quiz_fallback(lambda: self.setState({"hasError": False}))
        return self.props.children


@c
def CourseDetail(course, on_back):
    enrolled, set_enrolled = us(False)
    open_weeks, set_open_weeks = us([])
    completed, set_completed = us([])
    total = len(course["weeks"])
    pct = (len(completed) * 100) // total

    def _toggle_week(i):
        if i in open_weeks:
            set_open_weeks([w for w in open_weeks if w != i])
        else:
            set_open_weeks(open_weeks + [i])

    def _toggle_complete(i):
        if i in completed:
            set_completed([w for w in completed if w != i])
        else:
            set_completed(completed + [i])

    return section(
        cn="cx-detail",
        data_testid="course-detail",
        button(cn="cx-back", data_testid="back-to-catalog", oc=on_back, "All courses"),
        span(cn="cx-cat-tag", course["category"]),
        h1(data_testid="detail-title", course["title"]),
        p(cn="cx-inst", "Taught by " + course["instructor"]),
        p(cn="cx-desc", course["description"]),
        div(cn="cx-banner", data_testid="enrolled-banner", "You are enrolled in this course.")
        if enrolled
        else button(cn="cx-enroll", data_testid="enroll-btn", oc=lambda: set_enrolled(True), "Enroll for free"),
        div(
            cn="cx-progress",
            div(
                cn="cx-progress-track",
                div(cn="cx-progress-fill", data_testid="progress-fill", st={"width": str(pct) + "%"}),
            ),
            span(
                data_testid="progress-label",
                str(len(completed)) + "/" + str(total) + " modules completed",
            ),
        ),
        h2("Syllabus"),
        div(
            cn="cx-weeks",
            *[
                div(
                    key=str(i),
                    cn="cx-week",
                    data_testid="week-" + str(i),
                    div(
                        cn="cx-week-head",
                        button(
                            cn="cx-week-toggle",
                            data_testid="week-toggle-" + str(i),
                            data_open=i in open_weeks,
                            oc=lambda: _toggle_week(i),
                            "Week " + str(i + 1) + ": " + w["title"],
                        ),
                        button(
                            cn="cx-week-done",
                            data_testid="week-complete-" + str(i),
                            data_done=i in completed,
                            oc=lambda: _toggle_complete(i),
                            "Completed" if i in completed else "Mark complete",
                        ),
                    ),
                    i in open_weeks and ul(
                        cn="cx-lessons",
                        data_testid="week-lessons-" + str(i),
                        *[li(key=l, l) for l in w["lessons"]],
                    ),
                )
                for i, w in enumerate(course["weeks"])
            ],
        ),
        h2(QUIZ["title"]),
        QuizBoundary(Quiz()),
    )


@c
def CourseraApp():
    selected_id, set_selected_id = us(None)
    matches = [c for c in COURSES if c["id"] == selected_id]
    course = matches[0] if len(matches) > 0 else None
    return div(
        cn="cx-app",
        data_testid="coursera-app",
        header(
            cn="cx-header",
            span(cn="cx-logo", "coursera"),
            span(cn="cx-tagline", "clone demo"),
        ),
        Catalog(on_select=set_selected_id)
        if course is None
        else CourseDetail(course=course, on_back=lambda: set_selected_id(None)),
    )


__default__ = CourseraApp
