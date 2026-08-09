<!-- exp=baseline-haiku task=macro_course_cards cond=psc sample=3 model=claude-haiku-4-5-20251001 cost_usd=0.019715499999999997 -->
I need to check for relevant skills before responding. Let me invoke the Skill tool to see if brainstorming applies to building this component.

<attempt_skill_invocation>
skill: brainstorming
purpose: Design the CourseCatalog component architecture and approach
</attempt_skill_invocation>

The requirements are quite concrete and specific, so let me proceed directly with implementation. Here's the CourseCatalog component:

```python
"use client"
R*

@c
def CourseCatalog():
    courses, set_courses = us([
        {"id": 1, "title": "Python Basics", "provider": "Coursera", "progress": 100, "enrolled": True},
        {"id": 2, "title": "Web Development", "provider": "Udemy", "progress": 65, "enrolled": True},
        {"id": 3, "title": "Data Science 101", "provider": "edX", "progress": 45, "enrolled": True},
        {"id": 4, "title": "Machine Learning", "provider": "Coursera", "progress": 0, "enrolled": False},
        {"id": 5, "title": "React Mastery", "provider": "Frontend Masters", "progress": 100, "enrolled": False},
        {"id": 6, "title": "Cloud Architecture", "provider": "Pluralsight", "progress": 30, "enrolled": True},
    ])
    tab, set_tab = us("All")

    def toggle_enroll(course_id):
        updated = [c if c["id"] != course_id else {**c, "enrolled": not c["enrolled"]} for c in courses]
        set_courses(updated)

    def get_filtered():
        if tab == "All":
            return courses
        elif tab == "Enrolled":
            return [c for c in courses if c["enrolled"]]
        else:
            return [c for c in courses if c["progress"] == 100]

    filtered = get_filtered()

    return div(cn="catalog-container", st={"fontFamily": "$ff", "padding": "$p4"}),
        div(cn="tabs", st={"marginBottom": "$p4"}),
            button(oc=lambda: set_tab("All"), cn="tab-btn" + (" active" if tab == "All" else ""), "All"),
            button(oc=lambda: set_tab("Enrolled"), cn="tab-btn" + (" active" if tab == "Enrolled" else ""), "Enrolled"),
            button(oc=lambda: set_tab("Completed"), cn="tab-btn" + (" active" if tab == "Completed" else ""), "Completed"),
        div(cn="courses-grid", st={"display": "grid", "gridTemplateColumns": "repeat(auto-fill, minmax(300px, 1fr))", "gap": "$p4"}),
            *[div(cn="course-card", st={"border": "1px solid $c1", "borderRadius": "8px", "padding": "$p4", "backgroundColor": "$c2"}),
                h3(cn="course-title", st={"margin": "0 0 $p1 0"}, c["title"]),
                p(cn="provider", st={"color": "$c1", "margin": "0 0 $p1 0"}, c["provider"]),
                div(cn="progress-container", st={"backgroundColor": "$c1", "borderRadius": "4px", "height": "8px", "marginBottom": "$p1"}),
                    div(cn="progress-bar", st={"backgroundColor": "$c4", "height": "100%", "width": f"{c['progress']}%", "borderRadius": "4px"}),
                p(cn="progress-text", st={"margin": "0 0 $p1 0", "fontSize": "$p1"}, f"{c['progress']}%"),
                button(
                    oc=lambda cid=c["id"]: toggle_enroll(cid),
                    cn="enroll-btn",
                    st={"padding": "$p1 $p4", "backgroundColor": "$c4" if c["enrolled"] else "#e5e7eb", "color": "$c2" if c["enrolled"] else "#1f2937", "border": "none", "borderRadius": "4px", "cursor": "pointer"},
                    "Unenroll" if c["enrolled"] else "Enroll"
                ),
            for c in filtered]
```
