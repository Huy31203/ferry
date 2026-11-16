import { mount } from 'svelte';
import '../lib/theme.css';
import { applyAccent, applyOsClass } from '../lib/ipc';
import Shelf from './Shelf.svelte';

applyOsClass();
void applyAccent();
mount(Shelf, { target: document.getElementById('app')! });
