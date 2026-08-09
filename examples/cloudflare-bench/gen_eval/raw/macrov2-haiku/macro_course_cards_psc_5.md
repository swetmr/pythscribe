<!-- exp=macrov2-haiku task=macro_course_cards cond=psc sample=5 model=claude-haiku-4-5-20251001 cost_usd=0.041018 -->
```python
"use client"
R*

@c
def CourseCatalog():
    courses = [
        {"id": 1, "title": "Python Basics", "provider": "Coursera", "progress": 75},
        {"id": 2, "title": "React Fundamentals", "provider": "Udemy", "progress": 100},
        {"id": 3, "title": "JavaScript Advanced", "provider": "Coursera", "progress": 45},
        {"id": 4, "title": "Web Design Basics", "provider": "Skillshare", "progress": 100},
        {"id": 5, "title": "Database Design", "provider": "Udemy", "progress": 30},
        {"id": 6, "title": "DevOps Essentials", "provider": "Pluralsight", "progress": 60},
    ]
    
    enrolled, set_enrolled = us(set())
    tab, set_tab = us("All")
    
    def toggle_enroll(course_id):
        if course_id in enrolled:
            set_enrolled(enrolled - {course_id})
        else:
            set_enrolled(enrolled | {course_id})
    
    def get_filtered_courses():
        if tab == "All":
            return courses
        elif tab == "Enrolled":
            return [c for c in courses if c["id"] in enrolled]
        elif tab == "Completed":
            return [c for c in courses if c["progress"] == 100 and c["id"] in enrolled]
        return courses
    
    def make_toggle(cid):
        return lambda: toggle_enroll(cid)
    
    filtered = get_filtered_courses()
    
    return div(cn="course-catalog",
        div(cn="tabs",
            button(oc=lambda: set_tab("All"), "All"),
            button(oc=lambda: set_tab("Enrolled"), "Enrolled"),
            button(oc=lambda: set_tab("Completed"), "Completed"),
        ),
        div(cn="courses-container",
            *[div(cn="course-card", key=c["id"],
                h3(c["title"]),
                p(c["provider"]),
                div(cn="progress-bar",
                    div(cn="progress-fill", st={"width": f"{c['progress']}%"}),
                ),
                p(f"{c['progress']}%"),
                button(oc=make_toggle(c["id"]),
                       "Unenroll" if c["id"] in enrolled else "Enroll"),
            ) for c in filtered],
        ),
    )
```
