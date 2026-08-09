<!-- exp=baseline-haiku task=macro_course_cards cond=psc sample=5 model=claude-haiku-4-5-20251001 cost_usd=0.023365499999999997 -->
```python
"use client"
R*

@c
def CourseCatalog():
    courses_data = [
        {"id": 1, "title": "Python for Data Science", "provider": "Coursera", "progress": 45, "enrolled": False},
        {"id": 2, "title": "Machine Learning Basics", "provider": "edX", "progress": 100, "enrolled": True},
        {"id": 3, "title": "Web Development 101", "provider": "Coursera", "progress": 60, "enrolled": True},
        {"id": 4, "title": "Advanced JavaScript", "provider": "Udemy", "progress": 30, "enrolled": False},
        {"id": 5, "title": "React Mastery", "provider": "Coursera", "progress": 100, "enrolled": True},
        {"id": 6, "title": "Cloud Computing Essentials", "provider": "AWS", "progress": 75, "enrolled": True},
    ]
    
    courses, set_courses = us(courses_data)
    current_tab, set_current_tab = us("all")
    
    def toggle_enroll(course_id):
        updated = []
        for c in courses:
            if c["id"] == course_id:
                updated.append({**c, "enrolled": not c["enrolled"]})
            else:
                updated.append(c)
        set_courses(updated)
    
    def get_filtered_courses():
        if current_tab == "all":
            return courses
        elif current_tab == "enrolled":
            return [c for c in courses if c["enrolled"]]
        elif current_tab == "completed":
            return [c for c in courses if c["progress"] == 100]
        return []
    
    filtered = get_filtered_courses()
    
    return div(cn="catalog-container",
        div(cn="tabs",
            button(oc=lambda: set_current_tab("all"), cn="tab-btn" + (" active" if current_tab == "all" else ""), "All"),
            button(oc=lambda: set_current_tab("enrolled"), cn="tab-btn" + (" active" if current_tab == "enrolled" else ""), "Enrolled"),
            button(oc=lambda: set_current_tab("completed"), cn="tab-btn" + (" active" if current_tab == "completed" else ""), "Completed"),
        ),
        div(cn="courses-grid",
            *[div(cn="course-card", key=c["id"],
                h3(c["title"]),
                p(cn="provider", c["provider"]),
                div(cn="progress-container",
                    div(cn="progress-bar", st={"width": f"{c['progress']}%"}),
                ),
                p(cn="progress-text", f"{c['progress']}%"),
                button(
                    oc=lambda c_id=c["id"]: toggle_enroll(c_id),
                    cn="enroll-btn",
                    "Unenroll" if c["enrolled"] else "Enroll"
                ),
            ) for c in filtered]
        ),
    )
```
