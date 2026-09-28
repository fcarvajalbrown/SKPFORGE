Tenía una regla, y QuadriFlow no la cumplía.

En skpforge nada entra desde crates.io y toda librería en C++ se lee antes de entrar. QuadriFlow, la herramienta de retopología automática que necesitaba, depende de Boost y Eigen. Así que la porté a Rust.

Leerla completa sirvió. Encontré dos errores que la dejan colgada o sin resultado ante los muros en T de SketchUp. En una casa de prueba, el original seguía pegado cuando lo corté a los 600 segundos. El port la resuelve en 1,2.

¿Y el flujo máximo? Dinic, el de mejor cota, fue más lento. Boykov-Kolmogorov bajó esa etapa de 246 a 13,6 segundos.

La mejor cota perdió. Ganó medir.

El link está en la descripción.

#Rust #GameDev #UnrealEngine #SketchUp #Retopología

![Portada del artículo: a la izquierda, un toro de entrada de 8.100 triángulos; a la derecha, la retopología automática de Route B en 515 cuadriláteros](cover.png)
