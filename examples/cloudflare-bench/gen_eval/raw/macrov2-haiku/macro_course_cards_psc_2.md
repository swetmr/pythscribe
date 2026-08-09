<!-- exp=macrov2-haiku task=macro_course_cards cond=psc sample=2 model=claude-haiku-4-5-20251001 cost_usd=0.044018 -->
```python
"use client"
R*

COURSES = [
    {"id": 1, "title": "Python for Data Science", "provider": "Coursera", "progress": 45, "enrolled": False},
    {"id": 2, "title": "Web Development Basics", "provider": "Udemy", "progress": 100, "enrolled": True},
    {"id": 3, "title": "Machine Learning 101", "provider": "Coursera", "progress": 75, "enrolled": True},
    {"id": 4, "title": "Advanced JavaScript", "provider": "Pluralsight", "progress": 30, "enrolled": False},
    {"id": 5, "title": "React Mastery", "provider": "Udemy", "progress": 100, "enrolled": True},
    {"id": 6, "title": "Cloud Computing Essentials", "provider": "AWS Training", "progress": 60, "enrolled": True},
    {"id": 7, "title": "Data Structures & Algorithms", "provider": "MIT OpenCourseWare", "progress": 50, "enrolled": False},
]

@c
def CourseCard(course, on_toggle):
    return div(cn="course-card",
        h3(course["title"]),
        p(cn="provider", course["provider"]),
        div(cn="progress-container",
            div(cn="progress-bar", st={"width": f"{course['progress']}%"}),
        ),
        p(cn="progress-text", f"{course['progress']}% complete"),
        button(oc=lambda: on_toggle(course["id"]), 
               "Unenroll" if course["enrolled"] else "Enroll"),
    )

@c
def CourseCatalog():
    courses, set_courses = us(COURSES)
    tab, set_tab = us("All")
    
    def toggle_enrollment(course_id):
        updated = [
            {**c, "enrolled": not c["enrolled"]} if c["id"] == course_id else c
            for c in courses
        ]
        set_courses(updated)
    
    if tab == "All":
        filtered = courses
    elif tab == "Enrolled":
        filtered = [c for c in courses if c["enrolled"]]
    else:
        filtered = [c for c in courses if c["progress"] == 100]
    
    return div(cn="catalog",
        div(cn="tabs",
            button(oc=lambda: set_tab("All"), cn=("tab-btn active" if tab == "All" else "tab-btn"), "All"),
            button(oc=lambda: set_tab("Enrolled"), cn=("tab-btn active" if tab == "Enrolled" else "tab-btn"), "Enrolled"),
            button(oc=lambda: set_tab("Completed"), cn=("tab-btn active" if tab == "Completed" else "tab-btn"), "Completed"),
        ),
        div(cn="courses-grid",
            *[CourseCard(c, toggle_enrollment) for c in filtered],
        ),
    )
```
