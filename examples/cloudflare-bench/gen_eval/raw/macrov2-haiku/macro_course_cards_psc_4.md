<!-- exp=macrov2-haiku task=macro_course_cards cond=psc sample=4 model=claude-haiku-4-5-20251001 cost_usd=0.058433 -->
```python
"use client"
R*

@c
def CourseCatalog():
    courses = [
        {"id": 1, "title": "Python Basics", "provider": "Coursera", "progress": 75, "enrolled": True},
        {"id": 2, "title": "Web Development", "provider": "Udemy", "progress": 100, "enrolled": True},
        {"id": 3, "title": "Data Science 101", "provider": "edX", "progress": 45, "enrolled": False},
        {"id": 4, "title": "JavaScript Advanced", "provider": "Coursera", "progress": 60, "enrolled": True},
        {"id": 5, "title": "Machine Learning", "provider": "Udacity", "progress": 30, "enrolled": False},
        {"id": 6, "title": "React Mastery", "provider": "Pluralsight", "progress": 100, "enrolled": True},
    ]
    
    all_courses, set_all_courses = us(courses)
    active_tab, set_active_tab = us("All")
    
    def toggle_enroll(course_id):
        updated = [
            {**c, "enrolled": not c["enrolled"]} if c["id"] == course_id else c
            for c in all_courses
        ]
        set_all_courses(updated)
    
    def get_filtered_courses():
        if active_tab == "All":
            return all_courses
        elif active_tab == "Enrolled":
            return [c for c in all_courses if c["enrolled"]]
        elif active_tab == "Completed":
            return [c for c in all_courses if c["progress"] == 100]
        return []
    
    def make_enroll_handler(course_id):
        return lambda: toggle_enroll(course_id)
    
    filtered = get_filtered_courses()
    
    return div(cn="course-catalog",
        div(cn="tabs",
            button(oc=lambda: set_active_tab("All"), 
                   cn="tab-button" + (" active" if active_tab == "All" else ""), 
                   "All"),
            button(oc=lambda: set_active_tab("Enrolled"),
                   cn="tab-button" + (" active" if active_tab == "Enrolled" else ""),
                   "Enrolled"),
            button(oc=lambda: set_active_tab("Completed"),
                   cn="tab-button" + (" active" if active_tab == "Completed" else ""),
                   "Completed"),
        ),
        div(cn="courses-list",
            *[div(cn="course-card", key=c["id"],
                h3(c["title"]),
                p(cn="provider", c["provider"]),
                div(cn="progress-container",
                    div(cn="progress-bar", st={"width": f"{c['progress']}%"}),
                ),
                p(cn="progress-text", f"{c['progress']}%"),
                button(oc=make_enroll_handler(c["id"]),
                       cn="enroll-button",
                       "Unenroll" if c["enrolled"] else "Enroll"),
            ) for c in filtered]
        ),
    )
```
