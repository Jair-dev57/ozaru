import ozaru

for field in ozaru.form_fields("form.pdf"):
    print(f"{field.name}: {field.kind}")

values = {"name": "José Pérez", "agree": True, "country": "Chile", "langs": "French"}
ozaru.fill_form("form.pdf", "filled.pdf", values)

for field in ozaru.form_fields("filled.pdf"):
    if field.name in values:
        print(f"{field.name} = {field.value!r}")
