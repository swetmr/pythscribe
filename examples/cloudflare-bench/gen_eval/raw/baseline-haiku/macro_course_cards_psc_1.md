<!-- exp=baseline-haiku task=macro_course_cards cond=psc sample=1 model=claude-haiku-4-5-20251001 cost_usd=0.031959299999999996 -->
```python
"use client"
R*

def get_courses():
    return [
        {"id": 1, "title": "Python Basics", "provider": "Coursera", "progress": 85},
        {"id": 2, "title": "Advanced JavaScript", "provider": "Udemy", "progress": 45},
        {"id": 3, "title": "React Fundamentals", "provider": "Coursera", "progress": 100},
        {"id": 4, "title": "Data Science 101", "provider": "edX", "progress": 60},
        {"id": 5, "title": "Web Design Essentials", "provider": "Udemy", "progress": 20},
        {"id": 6, "title": "Machine Learning", "provider": "edX", "progress": 100},
    ]

@c
def CourseCatalog():
    courses = get_courses()
    current_tab, set_current_tab = us("all")
    enrolled_ids, set_enrolled_ids = us(set())
    
    def toggle_enroll(course_id):
        if course_id in enrolled_ids:
            set_enrolled_ids(enrolled_ids - {course_id})
        else:
            set_enrolled_ids(enrolled_ids | {course_id})
    
    def filter_courses():
        if current_tab == "all":
            return courses
        elif current_tab == "enrolled":
            return [c for c in courses if c["id"] in enrolled_ids]
        elif current_tab == "completed":
            return [c for c in courses if c["progress"] == 100 and c["id"] in enrolled_ids]
        return []
    
    filtered = filter_courses()
    
    return div(cn="catalog-container",
        div(cn="tabs",
            button(oc=lambda: set_current_tab("all"), cn=f"tab {'active' if current_tab == 'all' else ''}", "All"),
            button(oc=lambda: set_current_tab("enrolled"), cn=f"tab {'active' if current_tab == 'enrolled' else ''}", "Enrolled"),
            button(oc=lambda: set_current_tab("completed"), cn=f"tab {'active' if current_tab == 'completed' else ''}", "Completed"),
        ),
        div(cn="courses",
            *[div(cn="course-card", key=c["id"],
                h3(c["title"]),
                p(cn="provider", c["provider"]),
                div(cn="progress-container",
                    div(cn="progress-bar", st={"width": f"{c['progress']}%"}),
                ),
                p(cn="progress-text", f"{c['progress']}%"),
                button(
                    oc=lambda cid=c["id"]: toggle_enroll(cid),
                    cn="enroll-btn",
                    "Unenroll" if c["id"] in enrolled_ids else "Enroll"
                ),
            ) for c in filtered]
        ),
    )
```
