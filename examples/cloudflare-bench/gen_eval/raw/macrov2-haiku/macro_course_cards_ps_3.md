<!-- exp=macrov2-haiku task=macro_course_cards cond=ps sample=3 model=claude-haiku-4-5-20251001 cost_usd=0.020623499999999996 -->
```python
"use client"

from pyths.react import component, use_state

@component
def CourseCatalog():
    initial_courses = [
        {"id": 1, "title": "Python for Beginners", "provider": "Coursera", "progress": 45, "enrolled": True},
        {"id": 2, "title": "Web Development Basics", "provider": "Udemy", "progress": 100, "enrolled": True},
        {"id": 3, "title": "Data Science 101", "provider": "Coursera", "progress": 0, "enrolled": False},
        {"id": 4, "title": "Machine Learning Masters", "provider": "edX", "progress": 75, "enrolled": True},
        {"id": 5, "title": "JavaScript Advanced", "provider": "Udemy", "progress": 100, "enrolled": True},
        {"id": 6, "title": "Cloud Computing Essentials", "provider": "Coursera", "progress": 30, "enrolled": False},
        {"id": 7, "title": "Mobile App Development", "provider": "edX", "progress": 60, "enrolled": True},
    ]
    
    courses, set_courses = use_state(initial_courses)
    active_tab, set_active_tab = use_state("All")
    
    def toggle_enrollment(course_id):
        updated = []
        for course in courses:
            if course["id"] == course_id:
                updated.append({**course, "enrolled": not course["enrolled"]})
            else:
                updated.append(course)
        set_courses(updated)
    
    def filter_courses():
        if active_tab == "All":
            return courses
        elif active_tab == "Enrolled":
            return [c for c in courses if c["enrolled"]]
        else:
            return [c for c in courses if c["progress"] == 100]
    
    filtered = filter_courses()
    
    return div(class_name="course-catalog",
        div(class_name="catalog-tabs",
            button(
                class_name="tab-button" + (" tab-active" if active_tab == "All" else ""),
                on_click=lambda: set_active_tab("All"),
                "All"
            ),
            button(
                class_name="tab-button" + (" tab-active" if active_tab == "Enrolled" else ""),
                on_click=lambda: set_active_tab("Enrolled"),
                "Enrolled"
            ),
            button(
                class_name="tab-button" + (" tab-active" if active_tab == "Completed" else ""),
                on_click=lambda: set_active_tab("Completed"),
                "Completed"
            ),
        ),
        div(class_name="courses-container",
            *[
                div(class_name="course-card", key=c["id"],
                    h3(class_name="course-title", c["title"]),
                    p(class_name="course-provider", c["provider"]),
                    div(class_name="progress-bar-container",
                        div(class_name="progress-bar-fill",
                            style={"width": f"{c['progress']}%"}
                        ),
                    ),
                    p(class_name="progress-percentage", f"{c['progress']}%"),
                    button(
                        class_name="enroll-button",
                        on_click=lambda: toggle_enrollment(c["id"]),
                        "Unenroll" if c["enrolled"] else "Enroll"
                    ),
                )
                for c in filtered
            ]
        ),
    )
```
