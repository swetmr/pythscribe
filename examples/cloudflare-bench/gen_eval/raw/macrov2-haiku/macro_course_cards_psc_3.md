<!-- exp=macrov2-haiku task=macro_course_cards cond=psc sample=3 model=claude-haiku-4-5-20251001 cost_usd=0.03979800000000001 -->
```python
"use client"
R*

@c
def CourseCatalog():
    courses = [
        {"id": 1, "title": "Python Basics", "provider": "Coursera", "progress": 75},
        {"id": 2, "title": "Web Development", "provider": "Udemy", "progress": 100},
        {"id": 3, "title": "Data Science", "provider": "Coursera", "progress": 50},
        {"id": 4, "title": "Machine Learning", "provider": "edX", "progress": 25},
        {"id": 5, "title": "React Advanced", "provider": "Frontend Masters", "progress": 100},
        {"id": 6, "title": "AWS Cloud Essentials", "provider": "A Cloud Guru", "progress": 60},
    ]
    
    tab, set_tab = us("all")
    enrolled_ids, set_enrolled_ids = us({1, 2})
    
    if tab == "all":
        filtered = courses
    elif tab == "enrolled":
        filtered = [c for c in courses if c["id"] in enrolled_ids]
    else:
        filtered = [c for c in courses if c["progress"] == 100 and c["id"] in enrolled_ids]
    
    def toggle_enroll(course_id):
        new_enrolled = set(enrolled_ids)
        if course_id in new_enrolled:
            new_enrolled.discard(course_id)
        else:
            new_enrolled.add(course_id)
        set_enrolled_ids(new_enrolled)
    
    def mk_toggle(course_id):
        return lambda: toggle_enroll(course_id)
    
    return div(cn="course-catalog",
        div(cn="tabs",
            button(oc=lambda: set_tab("all"), "All"),
            button(oc=lambda: set_tab("enrolled"), "Enrolled"),
            button(oc=lambda: set_tab("completed"), "Completed"),
        ),
        div(cn="course-list",
            *[div(cn="course-card", key=c["id"],
                h3(c["title"]),
                p(cn="provider", c["provider"]),
                div(cn="progress-bar",
                    div(cn="progress-fill", st={"width": f"{c['progress']}%"}),
                ),
                p(cn="progress-text", f"{c['progress']}%"),
                button(oc=mk_toggle(c["id"]), "Unenroll" if c["id"] in enrolled_ids else "Enroll"),
            ) for c in filtered]
        ),
    )
```
