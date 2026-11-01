# ozaru

Extrae texto y cuenta las páginas de archivos PDF desde Python. El núcleo está
escrito en Rust y usa [pdf_oxide](https://crates.io/crates/pdf_oxide).

## Instalación

```bash
pip install ozaru
```

Requiere Python 3.10 o superior.

## Uso

```python
import ozaru

ozaru.page_count("documento.pdf")    # 3
ozaru.extract_text("documento.pdf")  # "Todo el texto del PDF..."
```

## Errores

| Excepción           | Cuándo ocurre                                          |
|---------------------|--------------------------------------------------------|
| `FileNotFoundError` | El archivo no existe.                                  |
| `ValueError`        | El archivo no se puede abrir o leer como un PDF válido. |

## Licencia

[MIT](https://github.com/Jair-dev57/ozaru/blob/main/LICENSE)
