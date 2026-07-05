#[cfg(not(feature = "ssr"))]
mod browser {
    use js_sys::wasm_bindgen::{JsCast as _, JsValue};
    use web_sys::{Document, Node};

    // 2. Le Wrapper Rust 100% typé (Safe Abstraction)
    pub fn raycast_text_node(doc: &Document, x: f32, y: f32) -> Option<(Node, usize)> {
        // Conversion du Document en valeur JS générique pour la réflexion
        let doc_val: &JsValue = doc.as_ref();
        let x_val = JsValue::from_f64(x as f64);
        let y_val = JsValue::from_f64(y as f64);

        // 1. TENTATIVE STANDARD W3C (Firefox)
        if let Ok(caret_pos_fn_val) =
            js_sys::Reflect::get(doc_val, &JsValue::from_str("caretPositionFromPoint"))
            && caret_pos_fn_val.is_function()
        {
            let caret_pos_fn: js_sys::Function = caret_pos_fn_val.unchecked_into();

            // Appel de la fonction : doc.caretPositionFromPoint(x, y)
            if let Ok(pos_obj) = caret_pos_fn.call2(doc_val, &x_val, &y_val)
                && !pos_obj.is_null()
                && !pos_obj.is_undefined()
            {
                let offset_node =
                    js_sys::Reflect::get(&pos_obj, &JsValue::from_str("offsetNode")).ok()?;
                let offset = js_sys::Reflect::get(&pos_obj, &JsValue::from_str("offset")).ok()?;

                let node: Node = offset_node.dyn_into().ok()?;
                return Some((node, offset.as_f64()? as usize));
            }
        }

        // 2. TENTATIVE WEBKIT/BLINK (Chrome, Edge, Safari)
        if let Ok(caret_range_fn_val) =
            js_sys::Reflect::get(doc_val, &JsValue::from_str("caretRangeFromPoint"))
            && caret_range_fn_val.is_function()
        {
            let caret_range_fn: js_sys::Function = caret_range_fn_val.unchecked_into();

            // Appel de la fonction : doc.caretRangeFromPoint(x, y)
            if let Ok(range_obj) = caret_range_fn.call2(doc_val, &x_val, &y_val)
                && !range_obj.is_null()
                && !range_obj.is_undefined()
            {
                let start_container =
                    js_sys::Reflect::get(&range_obj, &JsValue::from_str("startContainer")).ok()?;
                let start_offset =
                    js_sys::Reflect::get(&range_obj, &JsValue::from_str("startOffset")).ok()?;

                let node: Node = start_container.dyn_into().ok()?;
                return Some((node, start_offset.as_f64()? as usize));
            }
        }

        // Aucun raycast réussi ou clique hors du texte
        None
    }
}

#[cfg(feature = "ssr")]
mod server {
    use web_sys::{Document, Node};

    pub fn raycast_text_node(_doc: &Document, _x: f32, _y: f32) -> Option<(Node, usize)> {
        None
    }
}

#[cfg(feature = "ssr")]
pub use server::raycast_text_node;

#[cfg(not(feature = "ssr"))]
pub use browser::raycast_text_node;
