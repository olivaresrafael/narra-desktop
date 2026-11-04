---
name: metodos-estructura
description: Usa este skill para elegir y aplicar un método de estructura narrativa (Save the Cat, viaje del héroe, 8 secuencias, Story Circle, etc). Cada método tiene su propio archivo de referencia en metodos/.
---

# Métodos de estructura

## Objetivo
Dejar elegir al usuario un método de estructura narrativa, explicarlo
cuando lo pida, y aplicarlo para generar los beats/actos del proyecto
en `estructura/`.

## Métodos disponibles
Lee el archivo correspondiente en `metodos/` antes de explicar o
aplicar un método — no lo expliques de memoria sin confirmarlo:

- `metodos/save-the-cat.md`
- `metodos/viaje-del-heroe.md`
- `metodos/ocho-secuencias.md`
- `metodos/story-circle.md`
- `metodos/tres-actos-clasico.md`

Si el usuario pregunta "¿qué es X método?", lee el archivo y resume
en tus palabras — no reproduzcas el archivo completo, es solo tu
referencia.

## Proceso
1. Si el usuario no ha elegido método, muéstrale brevemente las
   opciones (una línea cada una) y pregunta cuál prefiere, o si
   quiere que le expliques alguno a fondo primero.
2. Una vez elegido, aplica los beats de ese método a la premisa y
   personajes ya definidos en la biblia.
3. Guarda el resultado en `estructura/metodo-elegido.md`:
   ```markdown
   # Método elegido: <nombre>

   ## Por qué este método
   ...

   ## Beats aplicados a este proyecto
   - Beat 1 (nombre del método): qué pasa en esta historia
   - Beat 2: ...
   ```
4. Este archivo alimenta después el skill `escaleta` (que desglosa
   cada beat en escenas concretas).

## Cómo agregar un método nuevo
Crea `metodos/<nombre>.md` con: qué es, de dónde viene, lista de
beats con una línea de descripción cada uno, y para qué tipo de
historia funciona mejor. Agrégalo a la lista de arriba.

## Qué NO hacer
- No fuerces una historia a encajar rígido en un método si el
  usuario prefiere usarlo solo como referencia libre — pregunta el
  nivel de rigidez que quiere.
