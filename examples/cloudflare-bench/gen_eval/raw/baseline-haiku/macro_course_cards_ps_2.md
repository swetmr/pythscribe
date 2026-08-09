<!-- exp=baseline-haiku task=macro_course_cards cond=ps sample=2 model=claude-haiku-4-5-20251001 cost_usd=0.041501300000000005 -->
```python
"use client"

from pyths.react import component, use_state

@component
def CourseCatalog():
    # Fixture data: 6+ courses with title, provider, progress (0-100)
    courses = [
        {"id": 1, "title": "Python for Everybody", "provider": "Coursera", "progress": 75},
        {"id": 2, "title": "Machine Learning Specialization", "provider": "DeepLearning.AI", "progress": 0},
        {"id": 3, "title": "Web Development Bootcamp", "provider": "Udacity", "progress": 100},
        {"id": 4, "title": "Data Science with R", "provider": "Coursera", "progress": 45},
        {"id": 5, "title": "Advanced Python Programming", "provider": "LinkedIn Learning", "progress": 90},
        {"id": 6, "title": "React Fundamentals", "provider": "Udacity", "progress": 100},
        {"id": 7, "title": "Cloud Computing Basics", "provider": "AWS Academy", "progress": 60},
    ]
    
    # State: enrolled course IDs and active tab
    enrolled_ids, set_enrolled_ids = use_state({1, 3, 4, 6})
    active_tab, set_active_tab = use_state("All")
    
    # Filter courses based on active tab
    def get_filtered_courses():
        filtered = []
        for course in courses:
            is_enrolled = course["id"] in enrolled_ids
            is_completed = course["progress"] == 100
            
            show = (active_tab == "All" or 
                    (active_tab == "Enrolled" and is_enrolled) or 
                    (active_tab == "Completed" and is_completed))
            
            if show:
                filtered.append(course)
        return filtered
    
    # Toggle enrollment status for a course
    def toggle_enroll(course_id):
        new_enrolled = set(enrolled_ids)
        if course_id in new_enrolled:
            new_enrolled.discard(course_id)
        else:
            new_enrolled.add(course_id)
        set_enrolled_ids(new_enrolled)
    
    filtered_courses = get_filtered_courses()
    
    return div(class_name="catalog-container",
        div(class_name="catalog-header",
            h1("Course Catalog"),
        ),
        div(class_name="tab-buttons",
            *[button(
                class_name=f"tab-button {'active' if active_tab == tab else ''}",
                on_click=lambda t=tab: set_active_tab(t),
                tab
            ) for tab in ["All", "Enrolled", "Completed"]]
        ),
        div(class_name="courses-container",
            *[div(class_name="course-card", key=course["id"],
                h3(class_name="course-title", course["title"]),
                p(class_name="course-provider", course["provider"]),
                div(class_name="progress-container",
                    div(class_name="progress-background",
                        div(class_name="progress-bar",
                            style={"width": f"{course['progress']}%"}
                        )
                    ),
                    span(class_name="progress-percent", f"{course['progress']}%"),
                ),
                button(
                    class_name="enroll-button",
                    on_click=lambda cid=course["id"]: toggle_enroll(cid),
                    "Unenroll" if course["id"] in enrolled_ids else "Enroll"
                ),
            ) for course in filtered_courses]
        ),
    )
```
