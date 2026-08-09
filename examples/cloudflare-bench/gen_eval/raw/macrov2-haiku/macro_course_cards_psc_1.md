<!-- exp=macrov2-haiku task=macro_course_cards cond=psc sample=1 model=claude-haiku-4-5-20251001 cost_usd=0.0682956 -->
```python
"use client"
R*

courses_data = [
    {"id": 1, "title": "Python for Data Science", "provider": "Coursera", "progress": 85},
    {"id": 2, "title": "Web Development Basics", "provider": "Udacity", "progress": 100},
    {"id": 3, "title": "Machine Learning 101", "provider": "Coursera", "progress": 45},
    {"id": 4, "title": "React Fundamentals", "provider": "Pluralsight", "progress": 60},
    {"id": 5, "title": "Cloud Computing Essentials", "provider": "Coursera", "progress": 100},
    {"id": 6, "title": "Advanced SQL", "provider": "DataCamp", "progress": 30},
]

@c
def CourseCatalog():
    enrolled_state, set_enrolled_state = us({c["id"]: False for c in courses_data})
    tab, set_tab = us("All")
    
    def toggle_enrolled(course_id):
        set_enrolled_state({**enrolled_state, course_id: not enrolled_state[course_id]})
    
    def mk_toggle(cid):
        return lambda: toggle_enrolled(cid)
    
    def get_filtered_courses():
        if tab == "All":
            return courses_data
        elif tab == "Enrolled":
            return [c for c in courses_data if enrolled_state[c["id"]]]
        elif tab == "Completed":
            return [c for c in courses_data if c["progress"] == 100 and enrolled_state[c["id"]]]
        return []
    
    filtered = get_filtered_courses()
    
    return div(cn="catalog-container",
        div(cn="tabs",
            button(oc=lambda: set_tab("All"), cn=f"tab-button{' active' if tab == 'All' else ''}", "All"),
            button(oc=lambda: set_tab("Enrolled"), cn=f"tab-button{' active' if tab == 'Enrolled' else ''}", "Enrolled"),
            button(oc=lambda: set_tab("Completed"), cn=f"tab-button{' active' if tab == 'Completed' else ''}", "Completed"),
        ),
        div(cn="courses-grid",
            *[div(cn="course-card", key=c["id"],
                h3(cn="course-title", c["title"]),
                p(cn="course-provider", c["provider"]),
                div(cn="progress-container",
                    div(cn="progress-bar", st={"width": f"{c['progress']}%"}),
                ),
                p(cn="progress-text", f"{c['progress']}% complete"),
                button(
                    oc=mk_toggle(c["id"]),
                    cn="enroll-button",
                    "Unenroll" if enrolled_state[c["id"]] else "Enroll"
                ),
            ) for c in filtered]
        ),
    )
```
