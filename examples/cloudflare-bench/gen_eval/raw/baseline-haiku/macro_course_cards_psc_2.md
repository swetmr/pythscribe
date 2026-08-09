<!-- exp=baseline-haiku task=macro_course_cards cond=psc sample=2 model=claude-haiku-4-5-20251001 cost_usd=0.0357755 -->
```python
"use client"
R*

@c
def CourseCatalog():
    all_courses = [
        {"id": 1, "title": "Python Basics", "provider": "CodeAcademy", "progress": 85},
        {"id": 2, "title": "Web Development", "provider": "Coursera", "progress": 100},
        {"id": 3, "title": "Machine Learning", "provider": "edX", "progress": 45},
        {"id": 4, "title": "React Fundamentals", "provider": "Pluralsight", "progress": 60},
        {"id": 5, "title": "Data Science", "provider": "Coursera", "progress": 100},
        {"id": 6, "title": "Cloud Computing", "provider": "Udemy", "progress": 30},
    ]
    
    enrolled_ids, set_enrolled_ids = us(set())
    current_tab, set_current_tab = us("All")
    
    def get_filtered_courses():
        if current_tab == "All":
            return all_courses
        elif current_tab == "Enrolled":
            return [c for c in all_courses if c["id"] in enrolled_ids]
        else:
            return [c for c in all_courses if c["progress"] == 100 and c["id"] in enrolled_ids]
    
    def toggle_enrolled(course_id):
        new_enrolled = set(enrolled_ids)
        if course_id in new_enrolled:
            new_enrolled.remove(course_id)
        else:
            new_enrolled.add(course_id)
        set_enrolled_ids(new_enrolled)
    
    filtered_courses = get_filtered_courses()
    
    return div(cn="course-catalog",
        div(cn="tabs",
            button(oc=lambda: set_current_tab("All"), cn="tab-btn", "All"),
            button(oc=lambda: set_current_tab("Enrolled"), cn="tab-btn", "Enrolled"),
            button(oc=lambda: set_current_tab("Completed"), cn="tab-btn", "Completed"),
        ),
        div(cn="course-list",
            *[div(cn="course-card", key=c["id"],
                h3(c["title"]),
                p(cn="provider", c["provider"]),
                div(cn="progress-bar-container",
                    div(cn="progress-bar", style={"width": f"{c['progress']}%"}),
                ),
                p(cn="progress-text", f"{c['progress']}%"),
                button(oc=lambda cid=c["id"]: toggle_enrolled(cid),
                       cn="enroll-btn",
                       "Unenroll" if c["id"] in enrolled_ids else "Enroll"),
            ) for c in filtered_courses],
        ),
    )
```
