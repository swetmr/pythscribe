<!-- exp=baseline-haiku task=macro_course_cards cond=psc sample=4 model=claude-haiku-4-5-20251001 cost_usd=0.026470499999999998 -->
```python
"use client"
R*

@c
def CourseCatalog():
    courses_data = [
        {"id": 1, "title": "Python Fundamentals", "provider": "Codecademy", "progress": 75, "enrolled": True},
        {"id": 2, "title": "React Essentials", "provider": "Coursera", "progress": 100, "enrolled": True},
        {"id": 3, "title": "Web Design Basics", "provider": "Udemy", "progress": 45, "enrolled": True},
        {"id": 4, "title": "Data Science 101", "provider": "edX", "progress": 0, "enrolled": False},
        {"id": 5, "title": "JavaScript Advanced", "provider": "Pluralsight", "progress": 60, "enrolled": True},
        {"id": 6, "title": "Cloud Computing", "provider": "LinkedIn Learning", "progress": 100, "enrolled": True},
    ]
    
    tab, set_tab = us("All")
    courses, set_courses = us(courses_data)
    
    def toggle_enrolled(course_id):
        updated = [c if c["id"] != course_id else {**c, "enrolled": not c["enrolled"]} for c in courses]
        set_courses(updated)
    
    def get_filtered_courses():
        if tab == "Enrolled":
            return [c for c in courses if c["enrolled"]]
        elif tab == "Completed":
            return [c for c in courses if c["progress"] == 100]
        else:
            return courses
    
    filtered = get_filtered_courses()
    
    return div(cn="catalog-container",
        div(cn="tabs",
            button(oc=lambda: set_tab("All"), cn="tab-btn" + (" active" if tab == "All" else ""), "All"),
            button(oc=lambda: set_tab("Enrolled"), cn="tab-btn" + (" active" if tab == "Enrolled" else ""), "Enrolled"),
            button(oc=lambda: set_tab("Completed"), cn="tab-btn" + (" active" if tab == "Completed" else ""), "Completed"),
        ),
        div(cn="courses-grid",
            *[div(cn="course-card", key=c["id"],
                h3(cn="course-title", c["title"]),
                p(cn="course-provider", c["provider"]),
                div(cn="progress-container",
                    div(cn="progress-bar", st={"width": f"{c['progress']}%"}),
                ),
                p(cn="progress-text", f"{c['progress']}%"),
                button(oc=lambda cid=c["id"]: toggle_enrolled(cid),
                       cn="enroll-btn",
                       "Unenroll" if c["enrolled"] else "Enroll"),
            ) for c in filtered]
        ),
    )
```
